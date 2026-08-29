interface LeasedContainer {
  id: string;
  name: string;
}

type InspectContainer = (
  command: string,
  args: string[],
  options: { encoding: 'utf8'; stdio: 'pipe'; timeout: number },
) => string;

export function assertLeasedContainer(
  container: LeasedContainer,
  exec: InspectContainer,
  timeoutMs: number,
  purpose: string,
): void {
  const actual = exec('docker', ['inspect', '--format', '{{.Id}}', container.name],
    { encoding: 'utf8', stdio: 'pipe', timeout: timeoutMs }).trim();
  if (actual !== container.id) {
    throw new Error(`${container.name} changed after lease creation; refusing ${purpose}`);
  }
}
