use std::time::Duration;

pub mod grpc_server;
pub mod http_server;
pub mod raft;

pub mod raft_proto {
    use serde::{ser::SerializeStruct, Serialize};

    tonic::include_proto!("raft"); // The string specified here must match the proto package name

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
            
            let value_str = String::from_utf8(self.value.clone()).unwrap_or("serialize_error".to_string());
            map.serialize_field("value", &value_str)?;

            map.end()
        }
    }
}

mod core;

pub type NodeId = i32;

const RAFT_APPEND_ENTRIES_INTERVAL: Duration = Duration::from_millis(100);
const RAFT_COMMIT_INTERVAL: Duration = Duration::from_millis(100);
const RAFT_COMMON_INTERVAL: Duration = Duration::from_millis(50);
