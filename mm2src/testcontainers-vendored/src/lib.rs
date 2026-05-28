// Vendored from artemii235/testcontainers-rs (fork of testcontainers-rs 0.7.0, MIT/Apache-2.0).
// Combines `tc_core`, `tc_cli_client`, `tc_generic` into one flat crate, preserving the
// `testcontainers::{clients::Cli, images::generic::{GenericImage, WaitFor}, Container, Docker, Image}`
// public API consumed by `mm2_main/src/docker_tests/`. See Cargo.toml for provenance.

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

mod container;
mod docker;
mod image;
mod wait_for_message;

pub use self::container::Container;
pub use self::docker::{Docker, Logs, Ports};
pub use self::image::Image;
pub use self::wait_for_message::{WaitError, WaitForMessage};

/// All available Docker clients.
pub mod clients;

/// All available Docker images.
pub mod images;
