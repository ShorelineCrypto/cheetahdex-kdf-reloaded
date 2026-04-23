//! Floodsub protocol customized for AtomicDEX — all peers are "target_peers".

use libp2p::core::PeerId;

pub mod protocol;

mod layer;
mod topic;

mod rpc_proto {
    include!(concat!(env!("OUT_DIR"), "/floodsub.pb.rs"));
}

pub use self::layer::{Floodsub, FloodsubEvent};
pub use self::protocol::{FloodsubMessage, FloodsubRpc};
pub use self::topic::Topic;

/// Configuration options for the Floodsub protocol.
pub struct FloodsubConfig {
    /// Peer id of the local node. Used for the source of the messages that we publish.
    pub local_peer_id: PeerId,

    /// `true` if messages published by local node should be propagated as messages received from
    /// the network, `false` by default.
    pub subscribe_local_messages: bool,

    pub forward_messages: bool,
}

impl FloodsubConfig {
    pub fn new(local_peer_id: PeerId, forward_messages: bool) -> Self {
        Self {
            local_peer_id,
            subscribe_local_messages: false,
            forward_messages,
        }
    }
}
