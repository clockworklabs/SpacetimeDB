import time
from .. import COMPOSE_FILE, Smoketest, run_cmd, requires_docker
from .zz_docker import restart_docker


def list_container():
    container_list = run_cmd("docker", "compose", "-f", COMPOSE_FILE, "ps", "--format", "{{.ID}} {{.Name}}")
    return container_list.splitlines() if container_list is not None else []

def kill_node_container(container_id):
    """
    Stop the first Docker container whose name contains the given substring.

    :param name_substr: Substring to match in container names
    """
    run_cmd("docker", "kill", container_id)



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
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = message, public)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    id: u64,
    text: String,
}

#[spacetimedb::reducer]
fn send_message(ctx: &ReducerContext, text: String) {
    ctx.db.message().insert(Message {id:0, text});
}

"""

#    def test_leader_failure(self):
#        """This test fails a leader, wait for new leader to be elected and verify if commits replicated to new leader"""
#
#        self.call("send_message", "hey")
#        leader = self.leader_node()
#        containers = list_container()
#        for container in containers:
#            if leader in container:
#                kill_node_container(container.split()[0])
#                break
#        time.sleep(2)
#
#        self.call("send_message", "joey")
#        
#        message_table = self.sql("SELECT * FROM message")
#        restart_docker()
#        time.sleep(2)
#        self.assertIn("hey", message_table)
#        self.assertIn("joey", message_table)
#

    def test_many_transactions(self):
        """This test sends many messages to the database and verifies that they are all present"""

        num_messages = 1000
        for i in range(num_messages+1):
            retry_on_error(lambda: self.call("send_message", f"{i}"))
        message_table = self.sql(f"SELECT text FROM message where text='{num_messages}'")
        self.assertIn("1000", message_table)


    def test_quorum_loss(self):
        """This test makes cluster to lose majority of followers to verify if leader eventually stop accepting writes"""

        for i in range(11):
            retry_on_error(lambda: self.call("send_message", f"{i}"))
        
        leader = self.leader_node()
        containers = list_container()
        for container in containers:
            if leader not in container and "worker" in container:
                kill_node_container(container.split()[0])

        time.sleep(2)
        for i in range(1001):
            if retry_on_error(lambda: self.call("send_message", f"{i}")) == False:
                break
            
        self.assertTrue(i > 1 and i < 1000, f"leader should stop accpeting writes between 1 and 1000, got {i}")


