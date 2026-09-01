import type { TextCommandExecutor } from '../runtime/command-executor.js';

interface LeasedContainer {
  id: string;
  name: string;
}

export function assertLeasedContainer(
  container: LeasedContainer,
  exec: TextCommandExecutor,
  timeoutMs: number,
  purpose: string,
): string {
  const actual = exec('docker', ['inspect', '--format', '{{.Id}}', container.name],
    { encoding: 'utf8', stdio: 'pipe', timeout: timeoutMs }).trim();
  if (actual !== container.id) {
    throw new Error(`${container.name} changed after lease creation; refusing ${purpose}`);
  }
  return actual;
}
