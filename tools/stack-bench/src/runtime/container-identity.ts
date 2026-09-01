import { execFileSync } from 'node:child_process';

export interface ContainerIdentity {
  name: string;
  id: string;
}

export function parseRunningContainerIdentity(name: string, output: string): ContainerIdentity {
  const [id, running] = output.trim().split(/\s+/);
  if (!id || !/^[a-f0-9]{64}$/.test(id) || running !== 'true') {
    throw new Error(`${name} is not a running Docker container`);
  }
  return { name, id };
}

export function runningContainerIdentity(name: string, timeoutMs = 120_000): ContainerIdentity {
  const output = execFileSync('docker',
    ['inspect', '--format', '{{.Id}} {{.State.Running}}', name],
    { encoding: 'utf8', stdio: 'pipe', timeout: timeoutMs });
  return parseRunningContainerIdentity(name, output);
}
