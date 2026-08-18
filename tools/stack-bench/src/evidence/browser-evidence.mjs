// Browser evidence helpers that preserve the difference between “not present”
// and “the page disappeared.” A timeout waiting for an optional element is a
// legitimate zero; any other Playwright failure must reach the harness-fault
// classifier instead of being coerced to zero.

export async function settledLocatorCount(locator, timeoutMs) {
  try {
    await locator.waitFor({ state: 'visible', timeout: timeoutMs });
  } catch (error) {
    if (error?.name !== 'TimeoutError') throw error;
  }
  return locator.count();
}
