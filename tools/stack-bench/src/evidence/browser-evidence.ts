// A missing optional element counts as zero. Other browser failures remain errors.

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
