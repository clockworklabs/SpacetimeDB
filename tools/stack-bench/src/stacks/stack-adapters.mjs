import { createStackAdapterRegistry, executeStackCapability }
  from './stack-adapter-contract.mjs';
import { mongodbAdapter } from './backends/mongodb-adapter.mjs';
import { postgresAdapter } from './backends/postgres-adapter.mjs';
import { spacetimeAdapter } from './backends/spacetime-adapter.mjs';
import { stubAdapter } from './backends/stub-adapter.mjs';

export { leasedDatabaseEnvironment } from './stack-adapter-common.mjs';

export const STACK_ADAPTER_REGISTRY = createStackAdapterRegistry([
  spacetimeAdapter,
  postgresAdapter,
  mongodbAdapter,
  stubAdapter,
]);

export function stackPortAllocations() {
  return Object.fromEntries(STACK_ADAPTER_REGISTRY.ids.map(id => [id,
    executeStackCapability(STACK_ADAPTER_REGISTRY.get(id), 'ports', 'allocations')]));
}
