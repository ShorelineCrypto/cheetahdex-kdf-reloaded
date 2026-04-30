mod event;
mod manager;
mod streamer;

pub use event::Event;
pub use manager::StreamingManager;
pub use streamer::{Broadcaster, EventStreamer, NoDataIn, StreamerId};

// Re-export channel types used in the EventStreamer trait so downstream
// crates don't need a direct tokio dependency (which is optional on WASM).
pub use tokio::sync::{mpsc, oneshot};
