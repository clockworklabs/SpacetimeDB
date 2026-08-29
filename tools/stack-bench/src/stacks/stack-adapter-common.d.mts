import type { StackAdapter } from './stack-adapter-contract.mjs';

export function leasedDatabaseEnvironment(
  adapter: StackAdapter,
  input: { database: string | null; networkMode: string | null | undefined },
): Record<string, string>;
