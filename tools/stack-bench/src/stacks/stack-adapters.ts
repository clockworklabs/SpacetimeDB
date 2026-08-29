import { createStackAdapterRegistry, executeStackCapability }
  from './stack-adapter-contract.js';
import { mongodbAdapter } from './backends/mongodb-adapter.js';
import { postgresAdapter } from './backends/postgres-adapter.js';
import { spacetimeAdapter } from './backends/spacetime-adapter.js';
import { stubAdapter } from './backends/stub-adapter.js';

export { leasedDatabaseEnvironment } from './stack-adapter-common.js';

export const STACK_ADAPTER_REGISTRY = createStackAdapterRegistry([
  spacetimeAdapter,
  postgresAdapter,
  mongodbAdapter,
  stubAdapter,
]);

export function stackPortAllocations(): Record<string, unknown> {
  return Object.fromEntries(STACK_ADAPTER_REGISTRY.ids.map(id => [id,
    executeStackCapability(STACK_ADAPTER_REGISTRY.get(id), 'ports', 'allocations')]));
}
