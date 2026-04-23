#![allow(dead_code)]
#![allow(clippy::all)]

//! AtomicDEX gossipsub — P2P pubsub routing with relay mesh maintenance.
//!
//! Custom gossipsub implementation derived from libp2p gossipsub, extended with
//! relay mesh maintenance, explicit relay lists, and `IncludedToRelaysMesh` control messages.

pub mod protocol;

mod behaviour;
mod config;
mod handler;
mod mcache;
mod topic;

mod rpc_proto {
    include!(concat!(env!("OUT_DIR"), "/gossipsub.pb.rs"));
}

pub use self::behaviour::{Gossipsub, GossipsubEvent, GossipsubRpc};
pub use self::config::{GossipsubConfig, GossipsubConfigBuilder};
pub use self::protocol::{GossipsubMessage, MessageId};
pub use self::topic::{Topic, TopicHash};
