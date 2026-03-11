import type { ConnectorKey } from '../connectors';

export type TestCase = {
  system: ConnectorKey;
  label?: string;
  run: (
    conn: unknown,
    from: number,
    to: number,
    amount: number,
  ) => Promise<void>;
};
export type TestCaseModule = { default: TestCase };
