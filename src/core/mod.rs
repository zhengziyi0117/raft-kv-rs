mod error;
mod node_status;

use std::{
    collections::HashMap, future::Future, hash::Hash, net::SocketAddr, path::Display, sync::Arc,
    time::Duration,
};

use error::RaftError;
use node_status::{
    RaftCandidateState, RaftFollowerState, RaftLeaderState, RaftNodeStatus, RaftStateMachine,
};
use rand::{rngs::ThreadRng, Rng, RngCore};
use tokio::{
    sync::{mpsc::UnboundedReceiver, oneshot::Sender},
    task::JoinHandle,
    time::Instant,
};
use tonic::{
    transport::{Channel, Uri},
    Response,
};

use crate::{
    raft_proto::{
        raft_service_client::RaftServiceClient, AppendEntriesArgs, AppendEntriesReply, Entry,
        RequestVoteArgs, RequestVoteReply,
    },
    raft_server::NodeId,
};

pub enum RaftMessage {
    AppendEntriesRequest(AppendEntriesArgs, Sender<AppendEntriesReply>),
    RequestVoteRequest(RequestVoteArgs, Sender<RequestVoteReply>),
    Shutdown,
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
        write!(f,"RaftCore:(me:{},current_term:{},voted_for:{:?},commit_index:{},last_applied:{},status:{:?},last_update_time:{:?})",
            self.me,self.current_term,self.voted_for,self.commit_index,self.last_applied,self.status,self.last_update_time)
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
            log::info!("{} raft server change status", self);
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

    pub(crate) async fn handle_append_entries(
        &mut self,
        args: AppendEntriesArgs,
    ) -> AppendEntriesReply {
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

    pub(crate) async fn handle_request_vote(&mut self, args: RequestVoteArgs) -> RequestVoteReply {
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

        // 可以投票的情况
        // todo 加上判断日志是否一样新
        if self.voted_for.is_none() || self.voted_for.is_some_and(|id| id == args.candidate_id) {
            self.last_update_time = Instant::now();
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

    pub(crate) fn to_follower(&mut self, term: i32, leader: Option<NodeId>) {
        self.current_term = term;
        self.status = RaftNodeStatus::Follower;
        self.voted_for = leader;
    }

    pub(crate) fn gen_next_elect_timeout() -> Duration {
        let mut rng = rand::thread_rng();
        let next_val = rng.gen_range(0..150);
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

    // // 请求
    // pub(crate) async fn send_request_vote(
    //     &mut self,
    //     peer: NodeId,
    //     args: RequestVoteArgs,
    // ) -> Result<RequestVoteReply, RaftError> {
    //     let channel = match self.peer2channel.get_mut(&peer) {
    //         Some(channel) => channel,
    //         None => {
    //             let uri = self.peer2uri.get(&peer).unwrap();
    //             let channel = RaftServiceClient::connect(uri.clone()).await?;
    //             self.peer2channel.insert(peer, channel);
    //             self.peer2channel.get_mut(&peer).unwrap()
    //         }
    //     };
    //     let resp = channel.request_vote(args).await?;
    //     Ok(resp.into_inner())
    // }

    // pub(crate) async fn send_append_entries(
    //     &mut self,
    //     peer: NodeId,
    //     args: AppendEntriesArgs,
    // ) -> Result<AppendEntriesReply, RaftError> {
    //     let channel = match self.peer2channel.get_mut(&peer) {
    //         Some(channel) => channel,
    //         None => {
    //             let uri = self.peer2uri.get(&peer).unwrap();
    //             let channel = RaftServiceClient::connect(uri.clone()).await?;
    //             self.peer2channel.insert(peer, channel);
    //             let channel = self.peer2channel.get_mut(&peer).unwrap();
    //             channel.clone()
    //         }
    //     };
    //     let resp = channel.append_entries(args).await?;
    //     Ok(resp.into_inner())
    // }
}
