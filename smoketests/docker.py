from dataclasses import dataclass
import os
import subprocess
import time
from typing import List, Optional
from urllib.request import urlopen
from . import COMPOSE_FILE
import json

def restart_docker():
    docker = DockerManager(COMPOSE_FILE)
    # Restart all containers.
    docker.compose("restart")
    # Ensure all nodes are reachable from outside.
    containers = docker.list_containers()
    for container in containers:
        info = json.loads(docker._execute_command("docker", "inspect", container.name))
        try:
            port = info[0]['NetworkSettings']['Ports']['80/tcp'][0]['HostPort']
        except KeyError:
            continue
        ping("127.0.0.1:{}".format(port))
    # TODO: ping endpoint needs to wait for database startup & leader election
    time.sleep(2)

def ping(host):
    tries = 0
    while tries < 10:
        tries += 1
        try:
            print(f"Ping Server at {host}")
            urlopen(f"http://{host}/v1/ping")
            print(f"Server up after {tries} tries")
            break
        except Exception:
            print("Server down")
            time.sleep(3)
    else:
        raise Exception(f"Server at {host} not responding")

@dataclass
class DockerContainer:
    """Represents a Docker container with its basic properties."""
    id: str
    name: str

class DockerManager:
    """Manages all Docker and Docker Compose operations."""

    def __init__(self, compose_file: str, **config):
        self.compose_file = compose_file
        self.network_name = config.get('network_name') or \
                           os.getenv('DOCKER_NETWORK_NAME', 'private_spacetime_cloud')
        self.control_db_container = config.get('control_db_container') or \
                                  os.getenv('CONTROL_DB_CONTAINER', 'node')
        self.spacetime_cli_bin = config.get('spacetime_cli_bin') or \
                                os.getenv('SPACETIME_CLI_BIN', 'spacetimedb-cloud')

    def _execute_command(self, *args: str) -> str:
        """Execute a Docker command and return its output."""
        try:
            result = subprocess.run(
                args,
                capture_output=True,
                text=True,
                check=True
            )
            return result.stdout.strip()
        except subprocess.CalledProcessError as e:
            print(f"Command failed: {e.stderr}")
            raise
        except Exception as e:
            print(f"Unexpected error: {str(e)}")
            raise

    def compose(self, *args: str) -> str:
        """Execute a docker-compose command."""
        return self._execute_command("docker", "compose", "-f", self.compose_file, *args)

    def list_containers(self) -> List[DockerContainer]:
        """List all containers and return as DockerContainer objects."""
        output = self.compose("ps", "-a", "--format", "{{.ID}} {{.Name}}")
        containers = []
        for line in output.splitlines():
            if line.strip():
                container_id, name = line.split(maxsplit=1)
                containers.append(DockerContainer(id=container_id, name=name))
        return containers

    def get_container_by_name(self, name: str) -> Optional[DockerContainer]:
        """Find a container by name pattern."""
        return next(
            (c for c in self.list_containers() if name in c.name),
            None
        )

    def kill_container(self, container_id: str):
        """Kill a container by ID."""
        print(f"Killing container {container_id}")
        self._execute_command("docker", "kill", container_id)

    def start_container(self, container_id: str):
        """Start a container by ID."""
        print(f"Starting container {container_id}")
        self._execute_command("docker", "start", container_id)

    def disconnect_container(self, container_id: str):
        """Disconnect a container from the network."""
        print(f"Disconnecting container {container_id}")
        self._execute_command(
            "docker", "network", "disconnect",
            self.network_name, container_id
        )
        print(f"Disconnected container {container_id}")

    def connect_container(self, container_id: str):
        """Connect a container to the network."""
        print(f"Connecting container {container_id}")
        self._execute_command(
            "docker", "network", "connect",
            self.network_name, container_id
        )
        print(f"Connected container {container_id}")

    def generate_root_token(self) -> str:
        """Generate a root token using spacetimedb-cloud."""
        return  self.compose(
            "exec", self.control_db_container, self.spacetime_cli_bin, "token", "gen",
            "--subject=placeholder-node-id",
            "--jwt-priv-key", "/etc/spacetimedb/keys/id_ecdsa").split('|')[1]
