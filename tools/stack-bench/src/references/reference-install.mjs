export function referenceInstallSteps(metadata) {
  return metadata.installDirectories.flatMap(directory => [
    ...(metadata.kind === 'spacetime' && metadata.frozenLock !== true ? [{
      directory, command: 'npm',
      args: ['install', '--package-lock-only', '--ignore-scripts', '--no-audit', '--no-fund'],
    }] : []),
    { directory, command: 'npm', args: ['ci', '--no-audit', '--no-fund'] },
  ]);
}
