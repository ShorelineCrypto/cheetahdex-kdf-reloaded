use crate::{Docker, Image, Logs};
use std::env::var;

/// Represents a running docker container.
///
/// Containers have a custom destructor that removes them as soon as they go out of scope.
#[derive(Debug)]
pub struct Container<'d, D, I>
where
    D: 'd,
    D: Docker,
    I: Image,
{
    id: String,
    docker_client: &'d D,
    image: I,
}

impl<'d, D, I> Container<'d, D, I>
where
    D: Docker,
    I: Image,
{
    pub fn new(id: String, docker_client: &'d D, image: I) -> Self {
        let container = Container {
            id,
            docker_client,
            image,
        };
        container.block_until_ready();
        container
    }

    pub fn id(&self) -> &str { &self.id }

    pub fn logs(&self) -> Logs { self.docker_client.logs(&self.id) }

    pub fn get_host_port(&self, internal_port: u32) -> Option<u32> {
        let resolved_port = self.docker_client.ports(&self.id).map_to_host_port(internal_port);
        match resolved_port {
            Some(port) => debug!("Resolved port {} to {} for container {}", internal_port, port, self.id),
            None => warn!("Unable to resolve port {} for container {}", internal_port, self.id),
        }
        resolved_port
    }

    pub fn image(&self) -> &I { &self.image }

    fn block_until_ready(&self) {
        debug!("Waiting for container {} to be ready", self.id);
        self.image.wait_until_ready(self);
        debug!("Container {} is now ready!", self.id);
    }

    fn stop(&self) {
        debug!("Stopping docker container {}", self.id);
        self.docker_client.stop(&self.id)
    }

    fn rm(&self) {
        debug!("Deleting docker container {}", self.id);
        self.docker_client.rm(&self.id)
    }
}

/// As soon as the container goes out of scope, the destructor will either only stop or delete the
/// docker container. Setting `KEEP_CONTAINERS=true` keeps containers (stop only).
impl<'d, D, I> Drop for Container<'d, D, I>
where
    D: Docker,
    I: Image,
{
    fn drop(&mut self) {
        let keep_container = var("KEEP_CONTAINERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(false);
        if keep_container {
            self.stop();
        } else {
            self.rm();
        }
    }
}
