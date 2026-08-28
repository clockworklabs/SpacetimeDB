export const CODING_CONTAINER_AGENT = Object.freeze({
  name: 'developer',
  uid: 10001,
  gid: 10001,
  home: '/home/developer',
});

export const CODING_CONTAINER_CONTROL_DIR = '/run/application';

export const CODING_CONTAINER_PROCESS_IDENTITY = Object.freeze({
  recordPrefix: '/tmp/developer-session-',
  sessionLabel: 'developer-session',
  stopLabel: 'developer-stop',
});

export function codingContainerAgentEnvironment() {
  return { HOME: CODING_CONTAINER_AGENT.home, USER: CODING_CONTAINER_AGENT.name };
}

export function codingContainerAgentExecOptions() {
  const agent = CODING_CONTAINER_AGENT;
  const environment = codingContainerAgentEnvironment();
  return ['--user', `${agent.uid}:${agent.gid}`, '-e', `HOME=${environment.HOME}`,
    '-e', `USER=${environment.USER}`];
}

export function codingContainerAgentCommand(command, args = []) {
  return ['sh', '-c', 'umask 000; exec "$@"', 'application-command', command, ...args];
}

// Claude creates transcript files with mode 0600. They are bind-mounted from
// the controller so the audit and cost ledger can inspect them after the coding
// session. Give the controller read access before the session container is
// released. Do not make transcripts writable by other users.
export function codingContainerTranscriptHandoffCommand() {
  return ['chmod', '-R', 'a+rX', `${CODING_CONTAINER_AGENT.home}/.claude/projects/-app`];
}
