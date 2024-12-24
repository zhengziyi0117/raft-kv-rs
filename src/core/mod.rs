mod candidate_state;
mod error;
mod follower_state;
mod leader_state;
use std::{cmp::min, collections::HashMap, net::SocketAddr, time::Duration};

use bytes::Bytes;
use candidate_state::RaftCandidateState;
use error::RaftError;
use follower_state::RaftFollowerState;
use leader_state::RaftLeaderState;
use rand::Rng;
use serde::Serialize;
use tokio::{
    sync::{broadcast::Receiver, mpsc::{UnboundedReceiver, UnboundedSender}, oneshot::Sender},
    task::JoinHandle,
    time::Instant,
};
use tonic::transport::{Channel, Uri};

use crate::{
    http_server::{
        RaftAddLogRequest, RaftAddLogResponse, RaftLogListResponse, RaftNodeStatusResponse,
    },
    raft_proto::{
        raft_service_client::RaftServiceClient, AppendEntriesArgs, AppendEntriesReply, CommandType,
        Entry, RequestVoteArgs, RequestVoteReply, EMPTY_ENTRY,
    },
    NodeId,
};

pub enum RaftMessage {
    // grpc api 不暴露给外部
    AppendEntriesRequest(AppendEntriesArgs, Sender<AppendEntriesReply>),
    RequestVoteRequest(RequestVoteArgs, Sender<RequestVoteReply>),

    // http api 暴露给外部
    GetStatusRequest(Sender<RaftNodeStatusResponse>),
    // 叫做add_log方便与内部rpc的append_entries区分
    AddLogRequest(RaftAddLogRequest, Sender<RaftAddLogResponse>),
    // 用来debug获取所有日志用
    ListLogsRequest(Sender<RaftLogListResponse>),
}

#[derive(Debug, PartialEq)]
pub enum RaftNodeStatus {
    Leader,
    Candidate,
    Follower,
    Shutdown,
}

pub trait RaftStateEventLoop {
    async fn event_loop(self);
}

pub trait RaftGrpcHandler {
    fn handle_append_entries(&mut self, args: AppendEntriesArgs) -> AppendEntriesReply;

    fn handle_request_vote(&mut self, args: RequestVoteArgs) -> RequestVoteReply;
}

pub trait RaftHttpHandle {
    fn handle_get_status(&self) -> RaftNodeStatusResponse;
    fn handle_list_logs(&self) -> RaftLogListResponse;

    fn handle_add_log(&mut self, request: RaftAddLogRequest) -> RaftAddLogResponse;
}

pub struct RaftCore {
    current_term: u32,
    voted_for: Option<NodeId>,
    logs: Vec<Entry>,
    // 所有server volatile
    commit_index: u32,
    last_applied: u32,
    // 非论文上的配置
    me: NodeId,
    status: RaftNodeStatus,
    peer2uri: HashMap<NodeId, Uri>,
    peer2channel: HashMap<NodeId, RaftServiceClient<Channel>>,
    last_update_time: Instant,
    next_elect_timeout: Duration,
    // 用于与grpc\http server通信
    msg_rx: UnboundedReceiver<RaftMessage>,
    shutdown: Receiver<()>,
    // 用于apply log
    apply_tx: UnboundedSender<Entry>,
}

impl std::fmt::Display for RaftCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // todo 增加打印log 最后的term和index
        let elapsed = self.last_update_time.elapsed();
        let timeout = elapsed.as_millis() as i64 - self.next_elect_timeout.as_millis() as i64;
        write!(f,"RaftCore:(me:{},current_term:{},voted_for:{:?},commit_index:{},last_applied:{},status:{:?},timeout:{})",
            self.me,self.current_term,self.voted_for,self.commit_index,self.last_applied,self.status,timeout)
    }
}

impl RaftCore {
    pub fn spawn(
        me: NodeId,
        peers: HashMap<NodeId, SocketAddr>,
        rx: UnboundedReceiver<RaftMessage>,
        shutdown: Receiver<()>,
        apply_tx: UnboundedSender<Entry>,
    ) -> JoinHandle<()> {
        let peer2uri = peers
            .iter()
            .map(|(k, v)| {
                // TODO scheme 换成 const
                let uri = Uri::builder()
                    .scheme("http")
                    .path_and_query("")
                    .authority(format!("{}:{}", v.ip(), v.port()))
                    .build()
                    .expect("peers addr error");
                (*k, uri)
            })
            .collect();

        let this = Self {
            current_term: 0,
            voted_for: None,
            logs: vec![EMPTY_ENTRY],
            commit_index: 0,
            last_applied: 0,
            me,
            status: RaftNodeStatus::Follower,
            peer2uri,
            peer2channel: HashMap::new(),
            msg_rx: rx,
            last_update_time: Instant::now(),
            next_elect_timeout: Self::gen_next_elect_timeout(),
            shutdown,
            apply_tx,
        };
        tokio::spawn(this.main())
    }

    // 这个event_loop用于消费所有事件，保证所有raft数据变更都是单线程执行
    async fn main(mut self) {
        loop {
            // TODO crash后恢复数据
            log::trace!("{} raft server change status", self);
            match self.status {
                RaftNodeStatus::Leader => RaftLeaderState::new(&mut self).event_loop().await,
                RaftNodeStatus::Candidate => RaftCandidateState::new(&mut self).event_loop().await,
                RaftNodeStatus::Follower => RaftFollowerState::new(&mut self).event_loop().await,
                RaftNodeStatus::Shutdown => {
                    log::info!("{} raft server shutdown!", self);
                    return;
                }
            }
        }
    }

    fn to_follower(&mut self, term: u32, leader: Option<NodeId>) {
        self.current_term = term;
        self.status = RaftNodeStatus::Follower;
        self.voted_for = leader;
    }

    fn gen_next_elect_timeout() -> Duration {
        let mut rng = rand::thread_rng();
        let next_val = rng.gen_range(0..150) + 150;
        Duration::from_millis(next_val)
    }

    fn is_elect_timeout(&self) -> bool {
        self.last_update_time.elapsed() > self.next_elect_timeout
    }

    async fn get_channel(
        &mut self,
        peer: &NodeId,
    ) -> Result<RaftServiceClient<Channel>, RaftError> {
        let channel = match self.peer2channel.get_mut(peer) {
            Some(channel) => channel.clone(),
            None => {
                let uri = self.peer2uri.get(peer).unwrap();
                let channel = RaftServiceClient::connect(uri.clone()).await?;
                self.peer2channel.insert(*peer, channel);
                let ch = self.peer2channel.get_mut(peer).unwrap();
                ch.clone()
            }
        };
        Ok(channel)
    }

    fn get_last_log_term_and_index(&self) -> (u32, u32) {
        let len = self.logs.len();
        let term = self.logs[len - 1].term;
        (term, (len - 1) as u32)
    }

    // 返回是否请求的日志 更新或者一样新
    fn is_request_log_newly(&self, request_log_term: u32, request_log_index: u32) -> bool {
        let (last_log_term, last_log_index) = self.get_last_log_term_and_index();
        if request_log_term != last_log_term {
            return request_log_term > last_log_term;
        } else {
            return request_log_index >= last_log_index;
        }
    }
}

impl RaftGrpcHandler for RaftCore {
    fn handle_append_entries(&mut self, args: AppendEntriesArgs) -> AppendEntriesReply {
        log::trace!("{} receive append_entries", self);

        if args.term < self.current_term {
            return AppendEntriesReply {
                term: self.current_term,
                success: false,
            };
        }

        self.last_update_time = Instant::now();
        self.to_follower(args.term, Some(args.leader_id));

        // 有日志
        let (_, mut last_log_index) = self.get_last_log_term_and_index();
        // 先考虑日志匹配的情况
        if last_log_index >= args.prev_log_index
            && self.logs[args.prev_log_index as usize].term == args.prev_log_term
        {
            // 心跳包 为没有entry, 在这里刚好处理了
            for entry in args.entries {
                // 如果日志不匹配
                if last_log_index >= entry.index
                    && self.logs[entry.index as usize].term != entry.term
                {
                    log::info!("{} truncate logs after {}", self, entry.index);
                    self.logs.truncate(entry.index as usize);
                }
                (_, last_log_index) = self.get_last_log_term_and_index();

                if entry.index == last_log_index+1 {
                    // TODO
                    log::info!("{} append logs {}", self, entry);
                    self.logs.push(entry);
                }
                (_, last_log_index) = self.get_last_log_term_and_index();
            }
            // 修改commit
            if args.leader_commit > self.commit_index {
                self.commit_index = min(args.leader_commit, last_log_index);
            }
            return AppendEntriesReply {
                term: self.current_term,
                success: true,
            };
        } else {
            return AppendEntriesReply {
                term: self.current_term,
                success: false,
            };
        }
    }

    fn handle_request_vote(&mut self, args: RequestVoteArgs) -> RequestVoteReply {
        log::trace!("{} receive request_vote", self);

        if args.term < self.current_term {
            return RequestVoteReply {
                term: self.current_term,
                vote_granted: false,
            };
        }

        if args.term > self.current_term {
            self.to_follower(args.term, None);
        }

        if (self.voted_for.is_none()
            && self.is_request_log_newly(args.last_log_term, args.last_log_index))
            || self.voted_for == Some(args.candidate_id)
        {
            self.voted_for = Some(args.candidate_id);
            log::trace!(
                "{} send request_vote true for peer:{}",
                self,
                args.candidate_id
            );
            RequestVoteReply {
                term: self.current_term,
                vote_granted: true,
            }
        } else {
            RequestVoteReply {
                term: self.current_term,
                vote_granted: false,
            }
        }
    }
}

impl RaftHttpHandle for RaftCore {
    fn handle_get_status(&self) -> RaftNodeStatusResponse {
        log::trace!("{} receive get_status", self);
        RaftNodeStatusResponse {
            current_term: self.current_term,
            is_leader: self.status == RaftNodeStatus::Leader,
        }
    }

    fn handle_add_log(&mut self, request: RaftAddLogRequest) -> RaftAddLogResponse {
        log::trace!("{} receive add_log", self);
        let (_, last_log_index) = self.get_last_log_term_and_index();
        if self.status != RaftNodeStatus::Leader {
            return RaftAddLogResponse {
                current_term: self.current_term,
                log_index: last_log_index,
                is_leader: false,
            };
        }
        // 这里在http接口校验过了
        let command_type = CommandType::from_str_name(&request.command_type).unwrap();
        let next_log_index = last_log_index+1;
        let entry = Entry {
            term: self.current_term,
            index: next_log_index,
            command_type: command_type.into(),
            key: request.key.clone(),
            value: Bytes::from(request.value).to_vec(),
        };
        self.logs.push(entry);
        RaftAddLogResponse {
            log_index: next_log_index,
            current_term: self.current_term,
            is_leader: true,
        }
    }

    fn handle_list_logs(&self) -> RaftLogListResponse {
        RaftLogListResponse {
            commit_index: self.commit_index,
            last_applied: self.last_applied,
            logs: self.logs.clone(),
        }
    }
}
