export interface ReferenceInstallMetadata {
  readonly kind: string;
  readonly frozenLock?: boolean;
  readonly installDirectories: readonly string[];
  readonly [key: string]: unknown;
}

export interface ReferenceInstallStep {
  readonly directory: string;
  readonly command: 'npm';
  readonly args: readonly string[];
}

export function referenceInstallSteps(
  metadata: ReferenceInstallMetadata,
): ReferenceInstallStep[] {
  return metadata.installDirectories.flatMap(directory => [
    ...(metadata.kind === 'spacetime' && metadata.frozenLock !== true ? [{
      directory, command: 'npm' as const,
      args: ['install', 'spacetimedb@file:/deps/spacetimedb.tgz', '--package-lock-only',
        '--ignore-scripts', '--no-audit', '--no-fund'],
    }] : []),
    { directory, command: 'npm' as const, args: ['ci', '--no-audit', '--no-fund'] },
  ]);
}
