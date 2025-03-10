use bollard::container::{
    AttachContainerOptions, Config, LogOutput, StartContainerOptions, WaitContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::HostConfig;
use bollard::Docker;
use futures::StreamExt;

pub struct ContainerConfig {
    pub image: String,
    pub env_vars: Vec<(String, String)>,
}

/// Runtime that uses the local Docker daemon to run containers.
pub struct LocalDockerRuntime {
    docker: Docker,
}

impl LocalDockerRuntime {
    /// Connect to the local Docker daemon.
    pub fn connect() -> anyhow::Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self { docker })
    }

    /// IP address to which services on the host should bind to be accessible from containers.
    pub async fn bridge_network_ip(&self) -> anyhow::Result<String> {
        // On Windows and macOS, services bound to "localhost" are not accessible from
        // containers via "host.docker.internal".
        if [os_info::Type::Windows, os_info::Type::Macos].contains(&os_info::get().os_type()) {
            return Ok("127.0.0.1".to_string());
        }

        // On Linux, services bound to "localhost" are not accessible from containers via "host.docker.internal".
        // Instead, we bind to the IP address of the Docker bridge network gateway.
        let network = self.docker.inspect_network::<&str>("bridge", None).await?;
        let ipam = network
            .ipam
            .ok_or_else(|| anyhow::anyhow!("Missing IPAM information in network inspection"))?;
        let configs = ipam
            .config
            .ok_or_else(|| anyhow::anyhow!("Missing IPAM configuration in network inspection"))?;
        let first_config = configs
            .first()
            .ok_or_else(|| anyhow::anyhow!("IPAM configuration list is empty"))?;
        let gateway = first_config
            .gateway
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Missing gateway in IPAM configuration"))?;

        Ok(gateway)
    }

    /// Pull a container image from a registry.
    pub async fn pull_container_image(&self, image: &str) -> anyhow::Result<()> {
        let options = Some(CreateImageOptions {
            from_image: image,
            ..Default::default()
        });

        let mut stream = self.docker.create_image(options, None, None);

        while let Some(result) = stream.next().await {
            result?;
        }

        Ok(())
    }

    /// Run a container with the given configuration.
    pub async fn run_container(&self, config: ContainerConfig) -> anyhow::Result<()> {
        let env: Vec<String> = config
            .env_vars
            .into_iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect();

        let host_config = HostConfig {
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            runtime: Some("sysbox-runc".to_string()),
            ..Default::default()
        };

        let container_config = Config {
            image: Some(config.image),
            env: Some(env),
            host_config: Some(host_config),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let container = self
            .docker
            .create_container::<&str, _>(None, container_config)
            .await?;
        self.docker
            .start_container(&container.id, None::<StartContainerOptions<String>>)
            .await?;

        let attach_options = Some(AttachContainerOptions::<&str> {
            stdout: Some(true),
            stderr: Some(true),
            stdin: None,
            stream: Some(true),
            logs: Some(true),
            ..Default::default()
        });

        let attached = self
            .docker
            .attach_container(&container.id, attach_options)
            .await?;

        let mut output_stream = attached.output;

        // Spawn a task to forward container output (stdout/stderr) to host stdout.
        let output_forwarder = tokio::spawn(async move {
            while let Some(Ok(log)) = output_stream.next().await {
                match log {
                    LogOutput::StdOut { message } => {
                        if let Ok(text) = String::from_utf8(message.to_vec()) {
                            print!("{}", text);
                        }
                    }
                    LogOutput::StdErr { message } => {
                        if let Ok(text) = String::from_utf8(message.to_vec()) {
                            eprint!("{}", text);
                        }
                    }
                    _ => {}
                }
            }
        });

        // Wait for the container to finish running.
        let mut wait_stream = self
            .docker
            .wait_container(&container.id, None::<WaitContainerOptions<String>>);

        if let Some(result) = wait_stream.next().await {
            let wait_msg = result?;
            if wait_msg.status_code > 0 {
                return Err(anyhow::anyhow!(
                    "Container exited with status code {}",
                    wait_msg.status_code
                ));
            }
        }

        let _ = output_forwarder.await;

        Ok(())
    }
}
