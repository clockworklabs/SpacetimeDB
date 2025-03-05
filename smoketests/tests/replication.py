import time, random, re
from .. import COMPOSE_FILE, STDB_DIR, TEST_DIR, Smoketest, run_cmd, requires_docker, spacetime
from .zz_docker import restart_docker

import os
from dataclasses import dataclass
from typing import List, Optional, Callable
import subprocess
import time
from functools import wraps

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

def retry(func: Callable, max_retries: int = 3, retry_delay: int = 2):
    """Retry a function on failure with delay."""
    for attempt in range(1, max_retries + 1):
        try:
            return func()
        except Exception as e:
            if attempt < max_retries:
                print(f"Attempt {attempt} failed: {e}. Retrying in {retry_delay} seconds...")
                time.sleep(retry_delay)
            else:
                print("Max retries reached. Skipping the exception.")
                return False

def get_int(text):
    return int(re.search(r'\d+', text).group())

class Cluster:
    """Manages leader-related operations and state for SpaceTime database cluster."""
    
    def __init__(self, docker_manager, smoketest: Smoketest):
        self.docker = docker_manager
        self.test = smoketest

    def read_controldb(self, sql):
        """Helper method to read from control database."""
        return self.test.spacetime("sql", "spacetime-control", sql)



    def get_db_id(self):
        # Query database ID
        sql = f"select id from database where database_identity=0x{self.test.database_identity}"
        db_id_tb = self.read_controldb(sql)
        return get_int(db_id_tb)


    def get_all_replicas(self):
        """Get all replica nodes in the cluster."""
        database_id = self.get_db_id()
        sql = f"select id, node_id from replica where database_id={database_id}"
        replica_tb = self.read_controldb(sql)
        replicas = []
        for line in replica_tb.splitlines()[2:]:
            replica_id, node_id = line.split('|')
            replicas.append({
                'replica_id': int(replica_id),
                'node_id': int(node_id)
            })
        return replicas

    def get_leader_info(self):
        """Get current leader's node information including ID, hostname, and container ID."""
        
        database_id = self.get_db_id()
        # Query leader replica ID
        sql = f"select leader from replication_state where database_id={database_id}"
        leader_tb = self.read_controldb(sql)
        leader_id = get_int(leader_tb)

        # Query leader node ID
        sql = f"select node_id from replica where id={leader_id}"
        leader_node_tb = self.read_controldb(sql)
        leader_node_id = get_int(leader_node_tb)

        # Query leader hostname
        sql = f"select network_addr from node where id={leader_node_id}"
        leader_host_tb = str(self.read_controldb(sql))
        lines = leader_host_tb.splitlines()
        
        hostname = ""
        if len(lines) == 3:  # actual row starts from 3rd line
            leader_row = lines[2]
            if "(some =" in leader_row:
                address = leader_row.split('"')[1]
                hostname = address.split(':')[0]

        # Find container ID
        container_id = ""
        containers = self.docker.list_containers()
        for container in containers:
            if hostname in container.name:
                container_id = container.id
                break

        return {
            'node_id': leader_node_id,
            'hostname': hostname,
            'container_id': container_id
        }

    def wait_for_leader_change(self, previous_leader_node, max_attempts=10, delay=2):
        """Wait for leader to change and return new leader node_id."""
        for _ in range(max_attempts):
            current_leader = self.get_leader_info()['node_id']
            if current_leader != previous_leader_node:
                return current_leader
            time.sleep(delay)
        return None

    def ensure_leader_health(self, id, wait_time=2):
        """Verify leader is healthy by inserting a row."""
        if wait_time:
            time.sleep(wait_time)

        retry(lambda: self.test.call("start", id, 1))
        add_table = str(self.test.sql(f"SELECT id FROM counter where id={id}"))
        if str(id) not in add_table:
            raise ValueError(f"Could not find {rnd} in counter table")


    def fail_leader(self, action='kill'):
        """Force leader failure through either killing or network disconnect."""
        leader_info = self.get_leader_info()
        container_id = leader_info['container_id']
        hostname = leader_info['hostname']

        if not container_id:
            raise ValueError("Could not find leader container")

        if action == 'kill':
            self.docker.kill_container(container_id)
        elif action == 'disconnect':
            self.docker.disconnect_container(container_id)
        else:
            raise ValueError(f"Unknown action: {action}")

        return container_id

    def restore_leader(self, container_id, action='start'):
        """Restore failed leader through either starting or network reconnect."""
        if action == 'start':
            self.docker.start_container(container_id)
        elif action == 'connect':
            self.docker.connect_container(container_id)
        else:
            raise ValueError(f"Unknown action: {action}")

@requires_docker
class ReplicationTest(Smoketest):
    MODULE_CODE = """
use spacetimedb::{duration, ReducerContext, Table};

#[spacetimedb::table(name = counter, public)]
pub struct Counter {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    value: u64,
}

#[spacetimedb::table(name = schedule_counter, public, scheduled(increment, at = sched_at))]
pub struct ScheduledCounter {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    sched_at: spacetimedb::ScheduleAt,
    count: u64,
}

#[spacetimedb::reducer]
fn increment(ctx: &ReducerContext, arg: ScheduledCounter) {
    // if the counter exists, increment it
    if let Some(counter) = ctx.db.counter().id().find(arg.scheduled_id) {
        if counter.value == arg.count {
            ctx.db.schedule_counter().delete(arg);
            return;
        }
        // update counter
        ctx.db.counter().id().update(Counter {
            id: arg.scheduled_id,
            value: counter.value + 1,
        });
    } else {
        // insert fresh counter
        ctx.db.counter().insert(Counter {
            id: arg.scheduled_id,
            value: 1,
        });
    }
}

#[spacetimedb::reducer]
fn start(ctx: &ReducerContext, id: u64, count: u64) {
    ctx.db.schedule_counter().insert(ScheduledCounter {
        scheduled_id: id,
        sched_at: duration!(0ms).into(),
        count,
    });
}
"""

    @classmethod
    def setUpClass(cls):
        super().setUpClass()
        cls.root_config = cls.project_path / "root_config"

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

        self.docker = DockerManager(COMPOSE_FILE)
        self.root_token = self.docker.generate_root_token()

        self.cluster = Cluster(self.docker, self)

    def add_me_as_admin(self):
        """Add the current user as an admin account"""
        db_owner_id = self.spacetime("login", "show").split()[-1]
        spacetime("--config-path", self.root_config, "login", "--token", self.root_token)
        spacetime("--config-path", self.root_config, "call", "spacetime-control", "create_admin_account", f"0x{db_owner_id}")

    def start(self, id: int, count: int):
        """Send a message to the database."""
        retry(lambda: self.call("start", id, count))

    def test_leader_election_in_loop(self):
        """This test fails a leader, wait for new leader to be elected and verify if commits replicated to new leader"""
        iterations = 5;
        row_ids = [101 + i for i in range(iterations * 2)]
        for (first_id, second_id) in zip(row_ids[::2], row_ids[1::2]):
            cur_leader = self.cluster.wait_for_leader_change(None)
            self.cluster.ensure_leader_health(first_id)

            print("killing current leader: {}", cur_leader)
            container_id = self.cluster.fail_leader()

            self.assertIsNotNone(container_id)
            
            next_leader = self.cluster.wait_for_leader_change(cur_leader)
            self.assertNotEqual(cur_leader, next_leader)
            # this check if leader election happened
            self.cluster.ensure_leader_health(second_id)
            # restart the old leader, so that we can maintain quorum for next iteration
            self.cluster.restore_leader(container_id, 'start')
        
        # verify if all past rows are present in new leader
        for row_id in row_ids:
            table = self.spacetime("sql", self.database_identity, f"SELECT * FROM counter WHERE id = {row_id}")
            self.assertIn(f"{row_id}", table)

    def test_leader_c_disconnect_in_loop(self):
        """This test disconnects a leader, wait for new leader to be elected and verify if commits replicated to new leader"""
        
        iterations = 5;
        row_ids = [201 + i for i in range(iterations * 2)]
            
        for (first_id, second_id) in zip(row_ids[::2], row_ids[1::2]):
            cur_leader = self.cluster.wait_for_leader_change(None)
            self.cluster.ensure_leader_health(first_id)

            container_id = self.cluster.fail_leader('disconnect')
            
            self.assertIsNotNone(container_id)

            next_leader = self.cluster.wait_for_leader_change(cur_leader)
            self.assertNotEqual(cur_leader, next_leader)
            # this check if leader election happened
            self.cluster.ensure_leader_health(second_id)
            
            # restart the old leader, so that we can maintain quorum for next iteration
            self.cluster.restore_leader(container_id, 'connect')
            time.sleep(1)

        # verify if all past rows are present in new leader
        for row_id in row_ids:
            table = self.spacetime("sql", self.database_identity, f"SELECT * FROM counter WHERE id = {row_id}")
            self.assertIn(f"{row_id}", table)


#    def test_drain_leader_node(self):
#        """This test moves leader replica to different node"""
#        self.add_me_as_admin()
#        cur_leader_node_id = self.cluster.wait_for_leader_change(None)
#        self.cluster.ensure_leader_health()
#
#        replicas = self.cluster.get_all_replicas()
#        empty_node_id = 14
#        for replica in replicas:
#            empty_node_id = empty_node_id - replica['node_id']
#        self.spacetime("call", "spacetime-control", "drain_node", f"{cur_leader_node_id}", f"{empty_node_id}")
#
#        time.sleep(5)
#        self.cluster.ensure_leader_health()
#        replicas = self.cluster.get_all_replicas()
#        for replica in replicas:
#            self.assertNotEqual(replica['node_id'], cur_leader_node_id)
#

    def test_prefer_leader(self):
        """This test moves leader replica to different node"""
        self.add_me_as_admin()
        cur_leader_node_id = self.cluster.wait_for_leader_change(None)
        self.cluster.ensure_leader_health(301)

        replicas = self.cluster.get_all_replicas()
        prefer_replica = {}
        for replica in replicas:
            if replica['node_id'] != cur_leader_node_id:
                prefer_replica = replica
                break
        prefer_replica_id = prefer_replica['replica_id']
        self.spacetime("call", "spacetime-control", "prefer_leader", f"{prefer_replica_id}")

        next_leader_node_id = self.cluster.wait_for_leader_change(cur_leader_node_id)
        self.cluster.ensure_leader_health(302)
        self.assertEqual(prefer_replica['node_id'], next_leader_node_id)


        # verify if all past rows are present in new leader
        for row_id in [301, 302]:
            table = self.spacetime("sql", self.database_identity, f"SELECT * FROM counter WHERE id = {row_id}")
            self.assertIn(f"{row_id}", table)


    def test_a_many_transactions(self):
        """This test sends many messages to the database and verifies that they are all present"""
        self.cluster.wait_for_leader_change(None)
        num_messages = 10000
        sub = self.subscribe("select * from counter", n=num_messages)
        self.start(1, num_messages)

        message_table = sub()[-1:];
        self.assertIn({'counter': {'deletes': [{'id': 1, 'value': 9999}], 'inserts': [{'id': 1, 'value': 10000}]}}, message_table)



#    def test_quorum_loss(self):
#        """This test makes cluster to lose majority of followers to verify if leader eventually stop accepting writes"""
#
#        for i in range(11):
#            retry_on_error(lambda: self.call("send_message", f"{i}"))
#        
#        leader = self.leader_node()
#        containers = list_container()
#        for container in containers:
#            if leader not in container and "worker" in container:
#                kill_node_container(container.split()[0])
#
#        time.sleep(2)
#        for i in range(1001):
#            if retry_on_error(lambda: self.call("send_message", f"{i}")) == False:
#                break
#            
#        self.assertTrue(i > 1 and i < 1000, f"leader should stop accpeting writes between 1 and 1000, got {i}")
#

