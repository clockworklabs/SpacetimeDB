import { parse as parseSemver } from 'semver';

export function isExactSemanticVersion(value: unknown): value is string {
  if (typeof value !== 'string') return false;
  const parsed = parseSemver(value);
  if (!parsed) return false;
  const canonical = `${parsed.version}${parsed.build.length ? `+${parsed.build.join('.')}` : ''}`;
  return value === canonical;
}

export function parseVersionedReference(value: unknown,
  isValidIdentifier: (identifier: string) => boolean): { id: string; version: string } | null {
  if (typeof value !== 'string') return null;
  const split = value.lastIndexOf('@');
  if (split < 1) return null;
  const id = value.slice(0, split);
  const version = value.slice(split + 1);
  return isValidIdentifier(id) && isExactSemanticVersion(version) ? { id, version } : null;
}
