import { posix } from 'node:path';

const VOLUME_NAME = /^[A-Za-z0-9][A-Za-z0-9_.-]*$/;

export interface ContainerMount {
  kind?: 'bind' | 'volume';
  source: string;
  target: string;
  readOnly: boolean;
}

export function dockerMountArguments(mount: unknown): string[] {
  if (!mount || typeof mount !== 'object' || Array.isArray(mount)) {
    throw new Error('container mount must be an object');
  }
  const candidate = mount as Record<string, unknown>;
  const keys = Object.keys(candidate).sort();
  const allowed = ['kind', 'readOnly', 'source', 'target'];
  if (keys.some(key => !allowed.includes(key))) {
    throw new Error(`container mount has unknown fields: ${keys.filter(key => !allowed.includes(key)).join(', ')}`);
  }
  const kind = candidate.kind ?? 'bind';
  if (typeof kind !== 'string' || !['bind', 'volume'].includes(kind)) {
    throw new Error(`container mount kind ${String(kind)} is unsupported`);
  }
  if (typeof candidate.source !== 'string' || !candidate.source.trim()) {
    throw new Error('container mount source must be a non-empty string');
  }
  if (typeof candidate.target !== 'string' || !posix.isAbsolute(candidate.target)
    || posix.normalize(candidate.target) !== candidate.target || candidate.target === '/') {
    throw new Error('container mount target must be a normalized absolute container path below /');
  }
  if (typeof candidate.readOnly !== 'boolean') throw new Error('container mount readOnly must be boolean');
  if (kind === 'volume' && !VOLUME_NAME.test(candidate.source)) {
    throw new Error(`container volume name ${candidate.source} is invalid`);
  }
  if (candidate.source.includes(',') || candidate.target.includes(',')) {
    throw new Error('container mount paths cannot contain commas');
  }
  const specification = [`type=${kind}`, `src=${candidate.source}`, `dst=${candidate.target}`];
  if (candidate.readOnly) specification.push('readonly');
  return ['--mount', specification.join(',')];
}
