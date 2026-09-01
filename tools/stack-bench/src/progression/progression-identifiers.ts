const IDENTIFIER = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;
const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

export const isProgressionIdentifier = (value: string): boolean => IDENTIFIER.test(value);
export function isProgressionVersion(value: string): boolean {
  const match = VERSION.exec(value);
  if (!match) return false;
  const prerelease = match[4];
  return prerelease === undefined
    || prerelease.split('.').every(part => !/^\d+$/.test(part) || part === '0' || !part.startsWith('0'));
}

export function parseVersionedProgressionId(value: string): { id: string; version: string } | null {
  const split = value.lastIndexOf('@');
  if (split < 1) return null;
  const id = value.slice(0, split);
  const version = value.slice(split + 1);
  return isProgressionIdentifier(id) && isProgressionVersion(version) ? { id, version } : null;
}
