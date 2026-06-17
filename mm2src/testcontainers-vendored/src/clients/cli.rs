use crate::{Container, Docker, Image, Logs};
use serde_json;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Instant;
use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    thread::sleep,
    time::Duration,
};

const ONE_SECOND: Duration = Duration::from_secs(1);
const ZERO: Duration = Duration::from_secs(0);

/// Implementation of the Docker client API using the docker cli.
#[derive(Debug, Default)]
pub struct Cli {
    container_startup_timestamps: RwLock<HashMap<String, Instant>>,
}

impl Cli {
    fn register_container_started(&self, id: String) {
        let mut lock_guard = match self.container_startup_timestamps.write() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        let start_timestamp = Instant::now();
        trace!("Registering starting of container {} at {:?}", id, start_timestamp);
        lock_guard.insert(id, start_timestamp);
    }

    fn time_since_container_was_started(&self, id: &str) -> Option<Duration> {
        let lock_guard = match self.container_startup_timestamps.read() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        let result = lock_guard.get(id).map(|i| Instant::now() - *i);
        trace!("Time since container {} was started: {:?}", id, result);
        result
    }

    fn wait_at_least_one_second_after_container_was_started(&self, id: &str) {
        if let Some(duration) = self.time_since_container_was_started(id) {
            if duration < ONE_SECOND {
                sleep(ONE_SECOND.checked_sub(duration).unwrap_or(ZERO))
            }
        }
    }

    fn build_run_command<'a, I: Image>(image: &I, command: &'a mut Command) -> &'a mut Command {
        command.arg("run");
        for (key, value) in image.env_vars() {
            command.arg("-e").arg(format!("{}={}", key, value));
        }
        command
            .arg("-d")
            .arg("-P")
            .args(image.args())
            .arg(image.descriptor())
            .stdout(Stdio::piped())
    }
}

impl Docker for Cli {
    #[allow(clippy::zombie_processes)]
    fn run<I: Image>(&self, image: I) -> Container<'_, Cli, I> {
        let mut docker = Command::new("docker");
        let command = Cli::build_run_command(&image, &mut docker);
        debug!("Executing command: {:?}", command);
        let child = command.spawn().expect("Failed to execute docker command");
        let stdout = child.stdout.unwrap();
        let reader = BufReader::new(stdout);
        let container_id = reader.lines().next().unwrap().unwrap();
        self.register_container_started(container_id.clone());
        Container::new(container_id, self, image)
    }

    #[allow(clippy::zombie_processes)]
    fn logs(&self, id: &str) -> Logs {
        self.wait_at_least_one_second_after_container_was_started(id);
        let child = Command::new("docker")
            .arg("logs")
            .arg("-f")
            .arg(id)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to execute docker command");
        Logs {
            stdout: Box::new(child.stdout.unwrap()),
            stderr: Box::new(child.stderr.unwrap()),
        }
    }

    #[allow(clippy::zombie_processes)]
    fn ports(&self, id: &str) -> crate::Ports {
        let child = Command::new("docker")
            .arg("inspect")
            .arg(id)
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to execute docker command");
        let stdout = child.stdout.unwrap();
        let mut infos: Vec<ContainerInfo> = serde_json::from_reader(stdout).unwrap();
        let info = infos.remove(0);
        trace!("Fetched container info: {:#?}", info);
        info.network_settings.ports.into_ports()
    }

    fn rm(&self, id: &str) {
        let mut child = Command::new("docker")
            .arg("rm")
            .arg("-f")
            .arg("-v")
            .arg(id)
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to execute docker command");
        child.wait().expect("Failed to wait for docker rm command");
    }

    fn stop(&self, id: &str) {
        let mut child = Command::new("docker")
            .arg("stop")
            .arg(id)
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to execute docker command");
        child.wait().expect("Failed to wait for docker stop command");
    }
}

#[derive(Deserialize, Debug)]
struct NetworkSettings {
    #[serde(rename = "Ports")]
    ports: Ports,
}

#[derive(Deserialize, Debug)]
struct PortMapping {
    #[serde(rename = "HostIp")]
    #[allow(dead_code)]
    ip: String,
    #[serde(rename = "HostPort")]
    port: String,
}

#[derive(Deserialize, Debug)]
struct ContainerInfo {
    #[serde(rename = "Id")]
    #[allow(dead_code)]
    id: String,
    #[serde(rename = "NetworkSettings")]
    network_settings: NetworkSettings,
}

#[derive(Deserialize, Debug)]
struct Ports(HashMap<String, Option<Vec<PortMapping>>>);

impl Ports {
    pub fn into_ports(self) -> crate::Ports {
        let mut ports = crate::Ports::default();
        for (internal, external) in self.0 {
            let external = match external.and_then(|mut m| m.pop()).map(|m| m.port) {
                Some(port) => port,
                None => {
                    debug!("Port {} is not mapped to host machine, skipping.", internal);
                    continue;
                },
            };
            let port = internal.split('/').next().unwrap();
            let internal = Self::parse_port(port);
            let external = Self::parse_port(&external);
            ports.add_mapping(internal, external);
        }
        ports
    }

    fn parse_port(port: &str) -> u32 {
        port.parse()
            .unwrap_or_else(|e| panic!("Failed to parse {} as u32 because {}", port, e))
    }
}
