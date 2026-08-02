use crate::targets::{util, ReleaseTarget};
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

pub struct DockerRelease {
    pub version: String,
    pub dry_run: bool,
}

struct LocalRegistryGuard {
    container_name: String,
}

impl LocalRegistryGuard {
    fn start(container_name: &str, host_port: u16) -> Result<Self, String> {
        println!("Starting local Docker registry for dry-run...");

        // Remove the container before creating, just in case there's a lingering copy from a previous run
        let mut cmd = Command::new("docker");
        cmd.args(["rm", "-f", container_name]);
        util::print_command(&cmd);
        let _ = cmd.status();

        // This registry:2 bit below just means that we're pulling version 2 from the official
        // docker "registry" image
        let mut cmd = Command::new("docker");
        cmd.args([
            "run",
            "-d",
            "--name",
            container_name,
            "-p",
            &format!("{}:5000", host_port),
            "registry:2",
        ]);
        util::print_command(&cmd);
        let status = cmd
            .status()
            .map_err(|e| format!("Failed to start local docker registry: {}", e))?;

        if !status.success() {
            return Err("Failed to start local docker registry".to_string());
        }

        Self::wait_until_ready(host_port)?;

        Ok(Self {
            container_name: container_name.to_string(),
        })
    }

    fn wait_until_ready(host_port: u16) -> Result<(), String> {
        let addr: SocketAddr = format!("127.0.0.1:{}", host_port)
            .parse()
            .map_err(|e| format!("Failed to parse registry address: {}", e))?;

        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(200));
        }

        Err(format!(
            "Timed out waiting for local docker registry to start at {}",
            addr
        ))
    }
}

impl Drop for LocalRegistryGuard {
    fn drop(&mut self) {
        println!("Stopping local Docker registry...");
        let mut cmd = Command::new("docker");
        cmd.args(["rm", "-f", &self.container_name]);
        util::print_command(&cmd);
        let _ = cmd.status();
    }
}

impl DockerRelease {
    pub fn new(version: String, dry_run: bool) -> Self {
        Self { version, dry_run }
    }

    /// Verify that docker is installed and user is logged in
    fn verify_docker_login(&self) -> Result<(), String> {
        println!("Verifying Docker login...");

        let mut cmd = Command::new("docker");
        cmd.arg("info");
        util::print_command(&cmd);
        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run docker command. Is Docker installed? Error: {}", e))?;

        if !output.status.success() {
            return Err("Docker is not running or you are not logged in. Please run 'docker login' first.".to_string());
        }

        println!("Docker is available and running.");
        Ok(())
    }

    /// Build the Docker image for multiple platforms and push to DockerHub
    fn build_and_push_image(&self, image_repo: &String) -> Result<(), String> {
        println!("\nBuilding Docker image for version: {}", self.version);
        println!("Building for platforms: linux/amd64, linux/arm64");

        let version_tag = format!("{}:{}", image_repo, self.version);

        // Build and push the multi-platform image
        let mut cmd = Command::new("docker");
        cmd.args([
            "buildx",
            "build",
            "--platform",
            "linux/amd64,linux/arm64",
            "-t",
            &version_tag,
            "--push",
            ".",
        ]);
        util::print_command(&cmd);
        let status = cmd
            .status()
            .map_err(|e| format!("Failed to execute docker buildx build: {}", e))?;

        if !status.success() {
            return Err(format!(
                "Failed to build and push Docker image for version {}",
                self.version
            ));
        }

        println!("Successfully built and pushed {}", version_tag);
        Ok(())
    }

    /// Tag the version as :latest
    fn tag_as_latest(&self, image_repo: &String) -> Result<(), String> {
        println!("\nTagging version {} as :latest", self.version);

        let mut cmd = Command::new("docker");
        cmd.args([
            "buildx",
            "imagetools",
            "create",
            "-t",
            &format!("{}:latest", image_repo),
            &format!("{}:{}", image_repo, self.version),
        ]);
        util::print_command(&cmd);
        let status = cmd
            .status()
            .map_err(|e| format!("Failed to execute docker buildx imagetools: {}", e))?;

        if !status.success() {
            return Err("Failed to tag image as :latest".to_string());
        }

        println!("Successfully tagged {}:latest", image_repo);
        Ok(())
    }
}

impl ReleaseTarget for DockerRelease {
    fn release(&self) -> Result<(), String> {
        let docker_repo_url = if self.dry_run {
            "localhost:5000/clockworklabs/spacetime"
        } else {
            "clockworklabs/spacetime"
        }
        .to_string();

        println!("=== Releasing Docker Container ===");
        println!("Version: {}", self.version);
        println!("Target: {}", &docker_repo_url);

        let _local_registry_guard = if self.dry_run {
            let container_name = format!("spacetimedb-release-local-registry-{}", std::process::id());
            Some(LocalRegistryGuard::start(&container_name, 5000)?)
        } else {
            None
        };

        if self.dry_run {
            println!("\n*** DRY RUN MODE - Skipping docker login ***\n");
        } else {
            // If we're pushing to a real repository, verify that we're logged in
            self.verify_docker_login()?;
        }

        self.build_and_push_image(&docker_repo_url)?;
        self.tag_as_latest(&docker_repo_url)?;

        println!("\n=== Docker Release Complete ===");
        println!("Images published:");
        println!("  - {}:{}", docker_repo_url, self.version);
        println!("  - {}:latest", docker_repo_url);

        Ok(())
    }

    fn name(&self) -> &'static str {
        "docker"
    }
}
