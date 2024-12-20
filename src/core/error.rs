use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum RaftError {
    Other(String),
    PeerNetworkError(String),
}

impl fmt::Display for RaftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RaftError::Other(err) => write!(f, "Other Error: {}", err),
            RaftError::PeerNetworkError(err) => write!(f, "PeerNetwork Error: {}", err),
        }
    }
}

impl Error for RaftError {}

impl From<std::io::Error> for RaftError {
    fn from(err: std::io::Error) -> RaftError {
        RaftError::PeerNetworkError(err.to_string())
    }
}

impl From<tonic::transport::Error> for RaftError {
    fn from(err: tonic::transport::Error) -> RaftError {
        RaftError::PeerNetworkError(err.to_string())
    }
}

impl From<tonic::Status> for RaftError {
    fn from(err: tonic::Status) -> RaftError {
        RaftError::PeerNetworkError(err.to_string())
    }
}
