use std::time::Duration;

mod grpc_server;
mod http_server;
pub mod raft;
pub mod fsm;


pub mod raft_proto {
    use serde::{ser::SerializeStruct, Serialize};
    use std::fmt::{write, Display};

    tonic::include_proto!("raft"); // The string specified here must match the proto package name

    pub const EMPTY_ENTRY: Entry = Entry {
        index: 0,
        term: 0,
        // TODO i32???
        command_type: CommandType::Empty as i32,
        key: String::new(),
        value: vec![],
    };

    impl Serialize for Entry {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            // 序列化为扁平的键值对
            let mut map = serializer.serialize_struct("Entry", 5)?;

            map.serialize_field("index", &self.index)?;
            map.serialize_field("term", &self.term)?;
            map.serialize_field("command_type", &self.command_type)?;
            map.serialize_field("key", &self.key)?;

            let value_str =
                String::from_utf8(self.value.clone()).unwrap_or("serialize_error".to_string());
            map.serialize_field("value", &value_str)?;

            map.end()
        }
    }

    impl Display for Entry {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "Entry(index:{},term:{},command_type:{},key:{},value:{})",
                self.index,
                self.term,
                self.command_type,
                self.key,
                String::from_utf8(self.value.clone()).unwrap_or("serialize_error".to_string())
            )
        }
    }
}

mod core;

pub type NodeId = i32;

const RAFT_APPEND_ENTRIES_INTERVAL: Duration = Duration::from_millis(100);
const RAFT_COMMIT_INTERVAL: Duration = Duration::from_millis(100);
const RAFT_COMMON_INTERVAL: Duration = Duration::from_millis(50);
const RAFT_ELECT_REQUEST_TIMEOUT: Duration = Duration::from_millis(300);
