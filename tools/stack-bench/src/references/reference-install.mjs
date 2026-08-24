export function referenceInstallSteps(metadata) {
  return metadata.installDirectories.flatMap(directory => {
    const steps = [];
    if (metadata.kind === 'spacetime') {
      steps.push({ directory, command: 'npm',
        args: ['install', '--package-lock-only', '--ignore-scripts', '--no-audit', '--no-fund'] });
    }
    steps.push({ directory, command: 'npm', args: ['ci', '--no-audit', '--no-fund'] });
    return steps;
  });
}
