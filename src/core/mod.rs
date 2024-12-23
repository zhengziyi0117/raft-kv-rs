mod candidate_state;
mod error;
mod follower_state;
mod leader_state;
use std::{collections::HashMap, net::SocketAddr, time::Duration};

use candidate_state::RaftCandidateState;
use error::RaftError;
use follower_state::RaftFollowerState;
use leader_state::RaftLeaderState;
use rand::Rng;
use tokio::{
    sync::{mpsc::UnboundedReceiver, oneshot::Sender},
    task::JoinHandle,
    time::Instant,
};
use tonic::transport::{Channel, Uri};

use crate::{
    http_server::RaftNodeStatusResponse,
    raft_proto::{
        raft_service_client::RaftServiceClient, AppendEntriesArgs, AppendEntriesReply, Entry,
        RequestVoteArgs, RequestVoteReply,
    },
    NodeId,
};

pub enum RaftMessage {
    // grpc api 不暴露给外部
    AppendEntriesRequest(AppendEntriesArgs, Sender<AppendEntriesReply>),
    RequestVoteRequest(RequestVoteArgs, Sender<RequestVoteReply>),

    // http api 暴露给外部
    GetStatusRequest(Sender<RaftNodeStatusResponse>),
    Shutdown,
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
    async fn handle_append_entries(&mut self, args: AppendEntriesArgs) -> AppendEntriesReply;

    async fn handle_request_vote(&mut self, args: RequestVoteArgs) -> RequestVoteReply;
}

pub trait RaftHttpHandle {
    fn handle_get_status(&self) -> RaftNodeStatusResponse;
}

pub struct RaftCore {
    current_term: i32,
    voted_for: Option<NodeId>,
    logs: Vec<Entry>,
    // 所有server volatile
    commit_index: i32,
    last_applied: i32,
    // 非论文上的配置
    me: NodeId,
    status: RaftNodeStatus,
    peer2uri: HashMap<NodeId, Uri>,
    peer2channel: HashMap<NodeId, RaftServiceClient<Channel>>,
    last_update_time: Instant,
    next_elect_timeout: Duration,
    // 用于与tonic server通信，处理rpc请求
    msg_rx: UnboundedReceiver<RaftMessage>,
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
            logs: vec![],
            commit_index: -1,
            last_applied: -1,
            me,
            status: RaftNodeStatus::Follower,
            peer2uri,
            peer2channel: HashMap::new(),
            msg_rx: rx,
            last_update_time: Instant::now(),
            next_elect_timeout: Self::gen_next_elect_timeout(),
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

    pub(crate) fn to_follower(&mut self, term: i32, leader: Option<NodeId>) {
        self.current_term = term;
        self.status = RaftNodeStatus::Follower;
        self.voted_for = leader;
    }

    pub(crate) fn gen_next_elect_timeout() -> Duration {
        let mut rng = rand::thread_rng();
        let next_val = rng.gen_range(0..150) + 150;
        Duration::from_millis(next_val)
    }

    pub(crate) fn is_elect_timeout(&self) -> bool {
        self.last_update_time.elapsed() > self.next_elect_timeout
    }

    pub(crate) async fn get_channel(
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
}

impl RaftGrpcHandler for RaftCore {
    async fn handle_append_entries(&mut self, args: AppendEntriesArgs) -> AppendEntriesReply {
        log::trace!("{} receive append_entries", self);

        if args.term < self.current_term {
            return AppendEntriesReply {
                term: self.current_term,
                success: false,
            };
        }

        self.last_update_time = Instant::now();
        self.to_follower(args.term, Some(args.leader_id));

        AppendEntriesReply {
            term: self.current_term,
            success: false,
        }
    }

    async fn handle_request_vote(&mut self, args: RequestVoteArgs) -> RequestVoteReply {
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

        if self.voted_for.is_none() || self.voted_for == Some(args.candidate_id) {
            self.voted_for = Some(args.candidate_id);
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
        RaftNodeStatusResponse {
            current_term: self.current_term,
            is_leader: self.status == RaftNodeStatus::Leader,
        }
    }
}
