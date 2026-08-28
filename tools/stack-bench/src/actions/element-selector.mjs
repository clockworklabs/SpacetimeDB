export function stableElementSelector(id) {
  if (!/^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/.test(id)) {
    throw new Error(`invalid stable element id ${JSON.stringify(id)}`);
  }
  return `[data-testid="${id}"],#${id}`;
}
