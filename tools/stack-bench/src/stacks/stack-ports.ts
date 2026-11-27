import type { StackPortBases } from './stack-adapter-contract.js';
import { stackIdentity } from './stack-identities.js';

// Port allocation has no runtime dependencies, so track loading can compute a
// run's ports without importing the adapters that themselves load tracks. Each
// stack declares its port bases in its identity module.

export function stackPorts(adapterId: string) {
  const allocations = stackIdentity(adapterId).ports;
  return {
    allocations: (): StackPortBases => ({ ...allocations }),
    forRun: ({ trackOffset, runIndex }: { trackOffset: number; runIndex: number }) => {
      if (!Number.isInteger(trackOffset) || trackOffset < 0
        || !Number.isInteger(runIndex) || runIndex < 0) {
        throw new Error(`${adapterId} ports require non-negative integer trackOffset and runIndex`);
      }
      const offset = trackOffset + runIndex;
      return {
        vite: allocations.vite + offset,
        express: allocations.express !== undefined ? allocations.express + offset : null,
        dbPort: allocations.db !== undefined ? allocations.db : null,
      };
    },
  };
}
