import { execFileSync } from 'node:child_process';

const EXACT_IMAGE_REFERENCE = /^[a-z0-9](?:[a-z0-9._:/-]*[a-z0-9])?@sha256:[0-9a-f]{64}$/;

export function isExactImageReference(value) {
  return typeof value === 'string' && EXACT_IMAGE_REFERENCE.test(value);
}

export function parseImageId(value) {
  const id = String(value).trim();
  if (!/^sha256:[0-9a-f]{64}$/.test(id)) {
    throw new Error(`Docker returned an invalid image content id: ${JSON.stringify(id)}`);
  }
  return id;
}

export function resolveContainerImage(reference, run = execFileSync) {
  if (!reference || typeof reference !== 'string') throw new Error('container image reference is required');
  const id = parseImageId(run('docker', ['image', 'inspect', '--format', '{{.Id}}', reference],
    { encoding: 'utf8', stdio: 'pipe' }));
  return { reference, id };
}
