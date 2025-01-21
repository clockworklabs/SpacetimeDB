import time
from .. import Smoketest, run_cmd, requires_docker
from .zz_docker import restart_docker


def kill_node_container(name_substr):
    """
    Stop the first Docker container whose name contains the given substring.

    :param name_substr: Substring to match in container names
    """
    container_list = run_cmd("docker", "ps", "--format", "{{.ID}} {{.Names}}")

    if container_list is None:
        return

    for line in container_list.splitlines():
        container_id, container_name = line.split(maxsplit=1)
        if name_substr in container_name:
            result = run_cmd("docker", "stop", container_id)
            if result is not None:
                print(f"Container '{container_name}' has been killed.")
            else:
                print(f"Failed to kill container '{container_name}'.")
            break


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
    def test_leader_failure(self):
        """This test fails a leader, wait for new leader to be elected and verify if commits replicated to new leader"""

        self.call("send_message", "hey")
        leader = self.leader_node();
        kill_node_container(leader)

        time.sleep(2)

        sub = self.subscribe("SELECT * FROM message", n=1)
        self.call("send_message", "joey")

        self.assertEqual(sub(), [{'scheduled_table': {'deletes': [], 'inserts': [{"id":1,"text":"hey"}, {"id":2,"text":"joey"}]}}])

