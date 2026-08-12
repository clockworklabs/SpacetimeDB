import { posix } from 'node:path';

const VOLUME_NAME = /^[A-Za-z0-9][A-Za-z0-9_.-]*$/;

export function dockerMountArguments(mount) {
  if (!mount || typeof mount !== 'object' || Array.isArray(mount)) {
    throw new Error('container mount must be an object');
  }
  const keys = Object.keys(mount).sort();
  const allowed = ['kind', 'readOnly', 'source', 'target'];
  if (keys.some(key => !allowed.includes(key))) {
    throw new Error(`container mount has unknown fields: ${keys.filter(key => !allowed.includes(key)).join(', ')}`);
  }
  const kind = mount.kind ?? 'bind';
  if (!['bind', 'volume'].includes(kind)) throw new Error(`container mount kind ${kind} is unsupported`);
  if (typeof mount.source !== 'string' || !mount.source.trim()) {
    throw new Error('container mount source must be a non-empty string');
  }
  if (typeof mount.target !== 'string' || !posix.isAbsolute(mount.target)
    || posix.normalize(mount.target) !== mount.target || mount.target === '/') {
    throw new Error('container mount target must be a normalized absolute container path below /');
  }
  if (typeof mount.readOnly !== 'boolean') throw new Error('container mount readOnly must be boolean');
  if (kind === 'volume' && !VOLUME_NAME.test(mount.source)) {
    throw new Error(`container volume name ${mount.source} is invalid`);
  }
  if (mount.source.includes(',') || mount.target.includes(',')) {
    throw new Error('container mount paths cannot contain commas');
  }
  const specification = [`type=${kind}`, `src=${mount.source}`, `dst=${mount.target}`];
  if (mount.readOnly) specification.push('readonly');
  return ['--mount', specification.join(',')];
}
