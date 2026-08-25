export function assertLeasedContainer(container, exec, timeoutMs, purpose) {
  const actual = exec('docker', ['inspect', '--format', '{{.Id}}', container.name],
    { encoding: 'utf8', stdio: 'pipe', timeout: timeoutMs }).trim();
  if (actual !== container.id) {
    throw new Error(`${container.name} changed after lease creation; refusing ${purpose}`);
  }
}
