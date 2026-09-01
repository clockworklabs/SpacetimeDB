export const CODING_CONTAINER_AGENT = Object.freeze({
  name: 'developer',
  uid: 10001,
  gid: 10001,
  home: '/home/developer',
});

export const CODING_CONTAINER_CONTROL_DIR = '/run/application';
export const CODING_CONTAINER_APP_ROOT = '/app';
export const CODING_CONTAINER_SPACETIME_CLI = '/deps/spacetimedb-cli';
export const CODING_CONTAINER_SPACETIME_STANDALONE = '/deps/spacetimedb-standalone';
export const CODING_CONTAINER_SPACETIME_PACKAGE = '/deps/spacetimedb.tgz';
export const CODING_CONTAINER_DEPENDENCY_READY_FILE = '/deps/.ready';
export const CODING_CONTAINER_RELEASE_DEPS_ROOT = '/release-deps';
export const CODING_CONTAINER_AGENT_CREDENTIAL_FILE = '/run/secrets/agent-credential';
export const CODING_CONTAINER_START_SCRIPT = `${CODING_CONTAINER_APP_ROOT}/start.sh`;
export const CODING_CONTAINER_BUG_REPORT_FILE = 'BUG_REPORT.md';

export const CODING_CONTAINER_PROCESS_IDENTITY = Object.freeze({
  recordPrefix: '/tmp/developer-session-',
  sessionLabel: 'developer-session',
  stopLabel: 'developer-stop',
});

export function codingContainerAgentEnvironment(): { HOME: string; USER: string } {
  return { HOME: CODING_CONTAINER_AGENT.home, USER: CODING_CONTAINER_AGENT.name };
}

export function codingContainerAgentExecOptions(): string[] {
  const agent = CODING_CONTAINER_AGENT;
  const environment = codingContainerAgentEnvironment();
  return ['--user', `${agent.uid}:${agent.gid}`, '-e', `HOME=${environment.HOME}`,
    '-e', `USER=${environment.USER}`];
}

export function codingContainerAgentCommand(command: string, args: readonly string[] = []): string[] {
  return ['sh', '-c', 'umask 022; exec "$@"', 'application-command', command, ...args];
}

// Give the controller read-only access to session transcripts after handoff.
export function codingContainerTranscriptHandoffCommand(): string[] {
  return ['chmod', '-R', 'a+rX', `${CODING_CONTAINER_AGENT.home}/.claude/projects/-app`];
}

export function codingContainerWorkspaceHandoffCommands(controllerGid: number): string[][] {
  return [
    ['chown', '-R', `${CODING_CONTAINER_AGENT.uid}:${controllerGid}`, CODING_CONTAINER_APP_ROOT],
    ['chmod', '-R', 'u+rwX,g+rwX,o-rwx', CODING_CONTAINER_APP_ROOT],
  ];
}
