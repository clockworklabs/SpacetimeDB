import json
import os
import subprocess
import time
from dataclasses import dataclass
from typing import List, Optional, Callable
from urllib.request import urlopen

from . import COMPOSE_FILE


def restart_docker():
    """
    Restart all containers defined in the current `COMPOSE_FILE`.

    Checks that all spacetimedb containers are up and running after the restart.
    If they're not up after a couple of retries, throws an `Exception`.
    """
    print("Restarting containers")

    docker = DockerManager(COMPOSE_FILE)
    docker.compose("restart")
    containers = docker.list_spacetimedb_containers()
    if not containers:
        raise Exception("No spacetimedb containers found")

    # Ensure all nodes are running.
    attempts = 0
    while attempts < 10:
        attempts += 1
        containers_alive = {
            container.name: container.is_running(docker, spacetimedb_ping_url)
            for container in containers
        }
        if all(containers_alive.values()):
            # sleep a bit more to allow for leader election etc
            # TODO: make ping endpoint consider all server state
            time.sleep(2)
            return
        else:
            time.sleep(1)

    raise Exception(f"Not all containers are up and running: {containers_alive!r}")

def spacetimedb_ping_url(port: int) -> str:
    return f"http://127.0.0.1:{port}/v1/ping"

@dataclass
class DockerContainer:
    """Represents a Docker container with its basic properties."""
    id: str
    name: str

    def host_ports(self, docker) -> set[int]:
        """
        Collect all host ports of this container.

        Host ports are ports on the host that are bound to ports of the
        container.
        If the container is not currently running, an empty set is returned.
        """
        host_ports = set()
        info = docker.inspect_container(self)
        for ports in info.get('NetworkSettings', {}).get('Ports', {}).values():
            if ports:
                for ip_and_port in ports:
                    host_port = ip_and_port.get("HostPort")
                    if host_port:
                        host_ports.add(host_port)
        return host_ports

    def is_running(self, docker, ping_url: Callable[[int], str]) -> bool:
        """
        Check if the container is running.

        `ping_url` takes a port number and returns a URL string that can be used
        to determine if the host is running by returning a 200 status.

        If `self.host_ports()` returns a non-empty set, and one `ping_url`
        request is successful, the container is considered running.
        """
        host_ports = self.host_ports(docker)
        for port in host_ports:
            url = ping_url(port)
            print(f"Trying {url} ... ", end='', flush=True)
            try:
                with urlopen(url, timeout=0.2) as response:
                    if response.status == 200:
                        print("ok")
                        return True
            except Exception as e:
                print(f"error: {e}")
                continue

        print(f"container {self.name} not running")
        return False

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
        """Execute a `docker compose` command."""
        return self._execute_command("docker", "compose", "-f", self.compose_file, *args)

    def docker(self, *args: str) -> str:
        """Execute a `docker` command."""
        return self._execute_command("docker", *args)

    def list_containers(self, *filters) -> List[DockerContainer]:
        """
        List the containers of the current compose file and return as DockerContainer objects.

        All containers are considered, even if not running ('-a' flag).
        The containers may be filtered by 'filters' ('--filter' option).
        """
        # Use -a so we don't miss a crashed or killed container
        # when checking for readiness.
        cmd = ["ps", "-a"]

        # Restrict to the current compose file.
        compose_file = os.path.abspath(COMPOSE_FILE)
        cmd.extend(["--filter", f"label=com.docker.compose.project.config_files={compose_file}"])

        # Apply additional filters.
        for f in filters:
            cmd.extend(["--filter", f])

        # Output only the fields we need for `DockerContainer`.
        cmd.extend(["--format", "{{.ID}} {{.Names}}"])

        output = self.docker(*cmd)
        containers = []
        for line in output.splitlines():
            if line.strip():
                container_id, name = line.split(maxsplit=1)
                containers.append(DockerContainer(id=container_id, name=name))
        return containers

    def list_spacetimedb_containers(self) -> List[DockerContainer]:
        """List all containers running spacetimedb."""
        return self.list_containers("label=app=spacetimedb")

    def inspect_container(self, container: DockerContainer):
        """Run the `inspect` command for `container`, returning the parsed JSON dict."""
        info = self.docker("inspect", container.name)
        return json.loads(info)[0]

    def get_container_by_name(self, name: str) -> Optional[DockerContainer]:
        """Find a container by name pattern."""
        return next(
            (c for c in self.list_containers() if name in c.name),
            None
        )

    def kill_container(self, container_id: str):
        """Kill a container by ID."""
        print(f"Killing container {container_id}")
        self.docker("kill", container_id)

    def start_container(self, container_id: str):
        """Start a container by ID."""
        print(f"Starting container {container_id}")
        self.docker("start", container_id)

    def disconnect_container(self, container_id: str):
        """Disconnect a container from the network."""
        print(f"Disconnecting container {container_id}")
        self.docker("network", "disconnect", self.network_name, container_id)
        print(f"Disconnected container {container_id}")

    def connect_container(self, container_id: str):
        """Connect a container to the network."""
        print(f"Connecting container {container_id}")
        self.docker("network", "connect", self.network_name, container_id)
        print(f"Connected container {container_id}")

    def generate_root_token(self) -> str:
        """Generate a root token using spacetimedb-cloud."""
        return  self.compose(
            "exec", self.control_db_container, self.spacetime_cli_bin, "token", "gen",
            "--subject=placeholder-node-id",
            "--jwt-priv-key", "/etc/spacetimedb/keys/id_ecdsa").split('|')[1]
