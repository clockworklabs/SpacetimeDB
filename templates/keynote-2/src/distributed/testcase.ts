import type { ConnectorKey } from '../connectors/index.ts';
import type { TestCase, TestCaseModule } from '../tests/types.ts';

export async function loadDistributedTestCase(
  testName: string,
  connector: ConnectorKey,
): Promise<TestCase> {
  const ref = new URL(`../tests/${testName}/${connector}.ts`, import.meta.url);
  let mod: TestCaseModule;
  try {
    mod = (await import(ref.href)) as TestCaseModule;
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    throw new Error(
      `Failed to load test case "${testName}" for connector "${connector}": ${msg}`,
    );
  }

  if (mod.default.system !== connector) {
    throw new Error(
      `Loaded test case "${testName}/${connector}" but its system is "${mod.default.system}"`,
    );
  }

  return mod.default;
}
