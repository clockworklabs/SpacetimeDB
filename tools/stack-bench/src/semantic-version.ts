import { parse as parseSemver } from 'semver';

export function isExactSemanticVersion(value: unknown): value is string {
  if (typeof value !== 'string') return false;
  const parsed = parseSemver(value);
  if (!parsed) return false;
  const canonical = `${parsed.version}${parsed.build.length ? `+${parsed.build.join('.')}` : ''}`;
  return value === canonical;
}
