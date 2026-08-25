export const releasePackages = [
  'spacetime-agents-ts',
  'spacetime-api-keys-ts',
  'spacetime-auth-ts',
  'spacetime-cron-ts',
  'spacetime-crypto-ts',
  'spacetime-files-ts',
  'spacetime-grid-ts',
  'spacetime-lobby-ts',
  'spacetime-posthog-ts',
  'spacetime-presence-ts',
  'spacetime-rate-limit-ts',
  'spacetime-resend-ts',
  'spacetime-retry-ts',
  'spacetime-stripe-ts',
];

export const spacetimedbVersion = '2.8.3';
export const spacetimedbPeerRange = 'workspace:^';
export const packedSpacetimedbPeerRange = '^2.8.3';

export function releasePackageName(packageDir) {
  const slug = packageDir.replace(/^spacetime-/, '').replace(/-ts$/, '');
  return `@spacetimedb/${slug}`;
}
