export function settledLocatorCount(
  locator: { count(): Promise<number> },
  within: number,
): Promise<number>;
