from .. import COMPOSE_FILE, Smoketest, requires_docker, spacetime
from ..docker import DockerManager

import time
from typing import Callable
import unittest

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

def parse_sql_result(res: str) -> list[dict]:
    """Parse tabular output from an SQL query into a list of dicts."""
    lines = res.splitlines()
    headers = lines[0].split('|') if '|' in lines[0] else [lines[0]]
    headers = [header.strip() for header in headers]
    rows = []
    for row in lines[2:]:
        cols = [col.strip() for col in row.split('|')]
        rows.append(dict(zip(headers, cols)))
    return rows

def int_vals(rows: list[dict]) -> list[dict]:
    """For all dicts in list, cast all values in dict to int."""
    return [{k: int(v) for k, v in row.items()} for row in rows]

class Cluster:
    """Manages leader-related operations and state for SpaceTime database cluster."""

    def __init__(self, docker_manager, smoketest: Smoketest):
        self.docker = docker_manager
        self.test = smoketest

        # Ensure all containers are up.
        self.docker.compose("up", "-d")

    def sql(self, sql: str) -> list[dict]:
        """Query the test database."""
        res = self.test.sql(sql)
        return parse_sql_result(str(res))

    def read_controldb(self, sql: str) -> list[dict]:
        """Query the control database."""
        res = self.test.spacetime("sql", "spacetime-control", sql)
        return parse_sql_result(str(res))

    def get_db_id(self):
        """Query database ID."""
        sql = f"select id from database where database_identity=0x{self.test.database_identity}"
        res = self.read_controldb(sql)
        return int(res[0]['id'])

    def get_all_replicas(self):
        """Get all replica nodes in the cluster."""
        database_id = self.get_db_id()
        sql = f"select id, node_id from replica where database_id={database_id}"
        return int_vals(self.read_controldb(sql))

    def get_leader_info(self):
        """Get current leader's node information including ID, hostname, and container ID."""

        database_id = self.get_db_id()
        sql = f""" \
select node_v2.id, node_v2.network_addr from node_v2 \
join replica on replica.node_id=node_v2.id \
join replication_state on replication_state.leader=replica.id \
where replication_state.database_id={database_id} \
"""
        rows = self.read_controldb(sql)
        if not rows:
            raise Exception("Could not find current leader's node")

        leader_node_id = int(rows[0]['id'])
        hostname = ""
        if "(some =" in rows[0]['network_addr']:
             address = rows[0]['network_addr'].split('"')[1]
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
            try:
                 current_leader_node = self.get_leader_info()['node_id']
                 if current_leader_node != previous_leader_node:
                    return current_leader_node
            except Exception:
                 print("No current leader")

            time.sleep(delay)
        return None

    def ensure_leader_health(self, id):
        """Verify leader is healthy by inserting a row."""

        retry(lambda: self.test.call("start", id, 1))
        rows = self.sql(f"select id from counter where id={id}")
        if len(rows) < 1 or int(rows[0]['id']) != id:
            raise ValueError(f"Could not find {id} in counter table")
        # Wait a tick to ensure buffers are flushed.
        # TODO: Replace with confirmed read.
        time.sleep(0.3)


    def fail_leader(self, action='kill'):
        """Force leader failure through either killing or network disconnect."""
        leader_info = self.get_leader_info()
        container_id = leader_info['container_id']

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

#[spacetimedb::table(name = message, public)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    id: u64,
    text: String
}

#[spacetimedb::reducer]
fn send_message(ctx: &ReducerContext, text: String) {
    ctx.db.message().insert(Message { id: 0, text });
}
"""

    @classmethod
    def setUpClass(cls):
        super().setUpClass()
        cls.root_config = cls.project_path / "root_config"

    def tearDown(self):
        # Ensure containers that were brought down during a test are back up.
        self.docker.compose("up", "-d")
        super().tearDown()

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

        self.docker = DockerManager(COMPOSE_FILE)
        self.root_token = self.docker.generate_root_token()

        self.cluster = Cluster(self.docker, self)

    def add_me_as_admin(self):
        """Add the current user as an admin account"""
        db_owner_id = str(self.spacetime("login", "show")).split()[-1]
        spacetime("--config-path", self.root_config, "login", "--token", self.root_token)
        spacetime("--config-path", self.root_config, "call", "spacetime-control", "create_admin_account", f"0x{db_owner_id}")

    def start(self, id: int, count: int):
        """Send a message to the database."""
        retry(lambda: self.call("start", id, count))

    def collect_counter_rows(self):
        return int_vals(self.cluster.sql("select * from counter"))


class LeaderElection(ReplicationTest):
    def test_leader_election_in_loop(self):
        """This test fails a leader, wait for new leader to be elected and verify if commits replicated to new leader"""
        iterations = 5
        row_ids = [101 + i for i in range(iterations * 2)]
        for (first_id, second_id) in zip(row_ids[::2], row_ids[1::2]):
            cur_leader = self.cluster.wait_for_leader_change(None)
            print(f"ensure leader health {first_id}")
            self.cluster.ensure_leader_health(first_id)

            print(f"killing current leader: {cur_leader}")
            container_id = self.cluster.fail_leader()

            self.assertIsNotNone(container_id)

            next_leader = self.cluster.wait_for_leader_change(cur_leader)
            self.assertNotEqual(cur_leader, next_leader)
            # this check if leader election happened
            print(f"ensure_leader_health {second_id}")
            self.cluster.ensure_leader_health(second_id)
            # restart the old leader, so that we can maintain quorum for next iteration
            print(f"reconnect leader {container_id}")
            self.cluster.restore_leader(container_id, 'start')

        # Ensure we have a current leader
        last_row_id = row_ids[-1] + 1
        self.cluster.ensure_leader_health(row_ids[-1] + 1)
        row_ids.append(last_row_id)

        # Verify that all inserted rows are present
        stored_row_ids = [row['id'] for row in self.collect_counter_rows()]
        self.assertEqual(set(stored_row_ids), set(row_ids))

class LeaderDisconnect(ReplicationTest):
    def test_leader_c_disconnect_in_loop(self):
        """This test disconnects a leader, wait for new leader to be elected and verify if commits replicated to new leader"""

        iterations = 5
        row_ids = [201 + i for i in range(iterations * 2)]

        for (first_id, second_id) in zip(row_ids[::2], row_ids[1::2]):
            print(f"first={first_id} second={second_id}")
            cur_leader = self.cluster.wait_for_leader_change(None)
            print(f"ensure leader health {first_id}")
            self.cluster.ensure_leader_health(first_id)

            print("disconnect current leader")
            container_id = self.cluster.fail_leader('disconnect')
            self.assertIsNotNone(container_id)
            print(f"disconnected leader's container is {container_id}")

            next_leader = self.cluster.wait_for_leader_change(cur_leader)
            self.assertNotEqual(cur_leader, next_leader)
            # this check if leader election happened
            print(f"ensure_leader_health {second_id}")
            self.cluster.ensure_leader_health(second_id)

            # restart the old leader, so that we can maintain quorum for next iteration
            print(f"reconnect leader {container_id}")
            self.cluster.restore_leader(container_id, 'connect')

        # Ensure we have a current leader
        last_row_id = row_ids[-1] + 1
        self.cluster.ensure_leader_health(last_row_id)
        row_ids.append(last_row_id)

        # Verify that all inserted rows are present
        stored_row_ids = [row['id'] for row in self.collect_counter_rows()]
        self.assertEqual(set(stored_row_ids), set(row_ids))


@unittest.skip("drain_node not yet supported")
class DrainLeader(ReplicationTest):
     def test_drain_leader_node(self):
         """This test moves leader replica to different node"""
         self.add_me_as_admin()
         cur_leader_node_id = self.cluster.wait_for_leader_change(None)
         self.cluster.ensure_leader_health(301)

         replicas = self.cluster.get_all_replicas()
         empty_node_id = 14
         for replica in replicas:
             empty_node_id = empty_node_id - replica['node_id']
         self.spacetime("call", "spacetime-control", "drain_node", f"{cur_leader_node_id}", f"{empty_node_id}")

         time.sleep(5)
         self.cluster.ensure_leader_health(302)
         replicas = self.cluster.get_all_replicas()
         for replica in replicas:
             self.assertNotEqual(replica['node_id'], cur_leader_node_id)


class PreferLeader(ReplicationTest):
    def test_prefer_leader(self):
        """This test moves leader replica to different node"""
        self.add_me_as_admin()
        cur_leader_node_id = self.cluster.wait_for_leader_change(None)
        self.cluster.ensure_leader_health(401)

        replicas = self.cluster.get_all_replicas()
        prefer_replica = {}
        for replica in replicas:
            if replica['node_id'] != cur_leader_node_id:
                prefer_replica = replica
                break
        prefer_replica_id = prefer_replica['id']
        self.spacetime("call", "spacetime-control", "prefer_leader", f"{prefer_replica_id}")

        next_leader_node_id = self.cluster.wait_for_leader_change(cur_leader_node_id)
        self.cluster.ensure_leader_health(402)
        self.assertEqual(prefer_replica['node_id'], next_leader_node_id)

        # verify if all past rows are present in new leader
        stored_row_ids = [row['id'] for row in self.collect_counter_rows()]
        self.assertEqual(set(stored_row_ids), set([401, 402]))


class ManyTransactions(ReplicationTest):
    def test_a_many_transactions(self):
        """This test sends many messages to the database and verifies that they are all present"""
        self.cluster.wait_for_leader_change(None)
        num_messages = 10000
        sub = self.subscribe("SELECT * FROM counter", n = num_messages)
        self.start(1, num_messages)

        message_table = sub()[-1:]
        self.assertIn({
            'counter': {
                'deletes': [{'id': 1, 'value': num_messages - 1}],
                'inserts': [{'id': 1, 'value': num_messages}]
            }
        }, message_table)



class QuorumLoss(ReplicationTest):
    def test_quorum_loss(self):
        """This test makes cluster to lose majority of followers to verify if leader eventually stop accepting writes"""

        for i in range(11):
            self.call("send_message", f"{i}")

        leader = self.cluster.get_leader_info()
        containers = self.docker.list_containers()
        for container in containers:
            if leader['container_id'] != container.id and "worker" in container.name:
                self.docker.kill_container(container.id)

        time.sleep(2)
        with self.assertRaises(Exception):
            for i in range(1001):
                self.call("send_message", "terminal")
