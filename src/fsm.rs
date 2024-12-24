use tokio::sync::{broadcast::{Receiver}, mpsc::UnboundedReceiver};

use crate::raft_proto::Entry;

pub trait FinishedStateMachine {
    fn apply(&mut self, entry: Entry);
}

pub async fn apply_logs(
    mut fsm: impl FinishedStateMachine,
    mut apply_rx: UnboundedReceiver<Entry>,
    mut shutdown: Receiver<()>,
) {
    loop {
        tokio::select! {
            entry = apply_rx.recv() => {
                if let Some(entry) = entry {
                    fsm.apply(entry);
                } else {
                    break;
                }
            }
            _ = shutdown.recv() => {
                break;
            }
        }
    }
}