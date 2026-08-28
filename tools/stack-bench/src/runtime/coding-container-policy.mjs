export const CODING_CONTAINER_AGENT = Object.freeze({
  name: 'stackbench',
  uid: 10001,
  gid: 10001,
  home: '/home/stackbench',
});

export const CODING_CONTAINER_CONTROL_DIR = '/run/stack-bench';

export function codingContainerAgentExecOptions() {
  const agent = CODING_CONTAINER_AGENT;
  return ['--user', `${agent.uid}:${agent.gid}`, '-e', `HOME=${agent.home}`,
    '-e', `USER=${agent.name}`];
}

export function codingContainerAgentCommand(command, args = []) {
  return ['sh', '-c', 'umask 000; exec "$@"', 'stack-bench-agent', command, ...args];
}
