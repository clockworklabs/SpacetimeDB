import time, random, re
from .. import COMPOSE_FILE, Smoketest, run_cmd, requires_docker
from .zz_docker import restart_docker

def list_container():
    container_list = run_cmd("docker", "compose", "-f", COMPOSE_FILE, "ps", "-a", "--format", "{{.ID}} {{.Name}}")
    return container_list.splitlines() if container_list is not None else []

def kill_container(container_id):
    run_cmd("docker", "kill", container_id)

def disconnect_container(container_id):
    print(f"Disconnecting container {container_id}")
    run_cmd("docker", "network", "disconnect", "private_spacetime_cloud", container_id)
    print(f"Disconnected container {container_id}")

def start_container(container_id):
    run_cmd("docker", "start", container_id)

def connect_container(container_id):
    print(f"Connecting container {container_id}")
    run_cmd("docker", "network", "connect", "private_spacetime_cloud", container_id)
    print(f"Connected container {container_id}")

def retry_on_error(func, max_retries=3, retry_delay=2):
    """Helper to retry a function on error."""
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


@requires_docker
class ReplicationTest(Smoketest):
    MODULE_CODE = """
use spacetimedb::{ReducerContext, Table, log};

#[spacetimedb::table(name = message, public)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    id: u64,
    text: String,
}

#[spacetimedb::reducer]
fn add(ctx: &ReducerContext, text: String) {
    log::info!("adding message: {}", text);
    ctx.db.message().insert(Message {id:0, text});
}


#[spacetimedb::reducer]
fn clean_up(ctx: &ReducerContext) {
    log::info!("cleaning up messages");
    ctx.db.message().iter().for_each(|m| {ctx.db.message().delete(m); });
}
"""

    def ensure_working_cluster(self, wait=False):
        """Ensure that the cluster is up and running."""
        if wait:
            time.sleep(2)

        rnd = random.randint(9000, 10000)
        retry_on_error(lambda: self.call("add", f"{rnd}"))
        add_table = self.sql(f"SELECT id, text FROM message where text='{rnd}'")
        self.assertIn(str(rnd), add_table)

    def add(self, r: range):
        """Send a message to the database."""
        for i in r:
            retry_on_error(lambda: self.call("add", f"{i}"))

    def count_rows(self):
        message_tb_raw = self.sql("SELECT id FROM message")
        # -2 to remove header
        return len(message_tb_raw.splitlines()) - 2


    def leader_node(self):
        """
        returns `network_addr` field of node which hosts leader replica of database
        `network_addr` is use to pattern match with container name
        """
        self._check_published()
        def get_int(text):
                return int(re.search(r'\d+', text).group())

        sql = f"select id from database where database_identity=0x{self.database_identity}"
        db_id_tb = self.read_controldb(sql)
        database_id = get_int(db_id_tb);


        sql = f"select leader from replication_state where database_id={database_id}"
        leader_tb = self.read_controldb(sql)
        leader_id = get_int(leader_tb)


        sql = f"select node_id from replica where id={leader_id}"
        leader_node_tb = self.read_controldb(sql)
        leader_node_id = get_int(leader_node_tb)

        sql = f"select network_addr from node where id={leader_node_id}"
        leader_host_tb = self.read_controldb(sql)
        lines = leader_host_tb.splitlines()
        
        # actual row starts from 3rd line
        if len(lines) != 3:
            return  None

        leader_row = lines[2]
        # Check if the line contains the network address
        if "(some =" in leader_row:
            address = leader_row.split('"')[1]
            hostname = address.split(':')[0]
            return hostname
        return None


    def get_leader_container_id(self):
        """Kill current leader container and return its"""
        leader = self.leader_node()
        containers = list_container()
        for container in containers:
            if leader in container:
                container_id = container.split()[0]
                return container_id
        return None

    def wait_for_leader_change(self, leader):
        """Wait for leader to change"""
        for i in range(10):
            time.sleep(2)
            next_leader = self.leader_node()
            if next_leader != leader:
                return next_leader
        return None

    def test_leader_election_in_loop(self):
        """This test fails a leader, wait for new leader to be elected and verify if commits replicated to new leader"""
        
        for i in range(5):
            cur_leader = self.wait_for_leader_change(None)
            self.ensure_working_cluster(True)
            print("killing current leader: {}", cur_leader)
            cur_leader_id = self.get_leader_container_id()
            kill_container(cur_leader_id)
            self.assertIsNotNone(cur_leader_id)
          
            next_leader = self.wait_for_leader_change(cur_leader)
            self.assertNotEqual(cur_leader, next_leader)
            # this check if leader election happened
            self.ensure_working_cluster(True)
            self.assertEqual(self.count_rows(), 2 * (i+1))
            # restart the old leader, so that we can maintain quorum for next iteration
            start_container(cur_leader_id)
        
        time.sleep(5)
        retry_on_error(lambda: self.call("clean_up"))
  
  
    def test_leader_disconnect_in_loop(self):
        """This test disconnects a leader, wait for new leader to be elected and verify if commits replicated to new leader"""
        
        for i in range(5):
            cur_leader = self.wait_for_leader_change(None)
            self.ensure_working_cluster(True)
            cur_leader_id = self.get_leader_container_id()
            disconnect_container(cur_leader_id)
            self.assertIsNotNone(cur_leader_id)

            next_leader = self.wait_for_leader_change(cur_leader)
            self.assertNotEqual(cur_leader, next_leader)
            # this check if leader election happened
            self.ensure_working_cluster(True)
            self.assertEqual(self.count_rows(), 2 * (i+1))
            
            # restart the old leader, so that we can maintain quorum for next iteration
            connect_container(cur_leader_id)
        
        time.sleep(5)
        retry_on_error(lambda: self.call("clean_up"))
    
#    def test_many_transactions(self):
#        """This test sends many messages to the database and verifies that they are all present"""
#        cur_leader = self.wait_for_leader_change(None)
#        num_messages = 1000
#        self.add(range(num_messages+1))
#        message_table = self.sql(f"SELECT text FROM message where text='{num_messages}'")
#        self.assertIn("1000", message_table)
#        self.assertEqual(self.count_rows(), num_messages)
#
#        retry_on_error(lambda: self.call("clean_up"))
#
#
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

