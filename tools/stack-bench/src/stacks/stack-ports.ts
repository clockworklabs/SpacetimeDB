import type { StackPortBases } from './stack-adapter-contract.js';

// Port allocation has no runtime dependencies, so track loading can compute a
// run's ports without importing the adapters that themselves load tracks.
const PORT_BASES = Object.freeze({
  spacetime: Object.freeze({ vite: 6173 }),
  postgres: Object.freeze({ vite: 6273, express: 6001, db: 6532 }),
  mongodb: Object.freeze({ vite: 6423, express: 6101, db: 6537 }),
  convex: Object.freeze({ vite: 6623, express: 6701 }),
  stub: Object.freeze({ vite: 7000 }),
});

export type StackPortId = keyof typeof PORT_BASES;

export function stackPorts(adapterId: string) {
  if (!Object.hasOwn(PORT_BASES, adapterId)) throw new Error(`unknown stack adapter ${adapterId}`);
  const allocations = PORT_BASES[adapterId as StackPortId];
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
        express: 'express' in allocations ? allocations.express + offset : null,
        dbPort: 'db' in allocations ? allocations.db : null,
      };
    },
  };
}
