export function stableElementSelector(name: string): string {
  if (!/^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/.test(name)) {
    throw new Error(`invalid application interface name ${JSON.stringify(name)}`);
  }
  return `[data-role="${name}"],#${name}`;
}
