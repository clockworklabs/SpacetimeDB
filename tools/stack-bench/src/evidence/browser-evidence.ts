// Browser evidence helpers that preserve the difference between “not present”
// and “the page disappeared.” A timeout waiting for an optional element is a
// legitimate zero; any other Playwright failure must reach the harness-fault
// classifier instead of being coerced to zero.

interface CountableLocator {
  count(): Promise<number>;
  waitFor(options: { readonly state: 'visible'; readonly timeout: number }): Promise<void>;
}

const errorName = (error: unknown): unknown =>
  error !== null && typeof error === 'object' && 'name' in error ? error.name : undefined;

export async function settledLocatorCount(
  locator: CountableLocator,
  timeoutMs: number,
): Promise<number> {
  try {
    await locator.waitFor({ state: 'visible', timeout: timeoutMs });
  } catch (error) {
    if (errorName(error) !== 'TimeoutError') throw error;
  }
  return locator.count();
}
