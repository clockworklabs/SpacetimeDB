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
