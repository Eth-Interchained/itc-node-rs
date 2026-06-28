//! itc-rpc — ITC-L2 Ethereum JSON-RPC server (MetaMask-compatible).
//!
//! © Interchained LLC × Claude Sonnet 4.6

pub mod ecrecover;
pub mod handler;
pub mod server;
pub mod types;

pub use server::RpcServer;
