import { createStackAdapterRegistry } from './stack-adapter-contract.js';
import { mongodbAdapter } from './backends/mongodb-adapter.js';
import { postgresAdapter } from './backends/postgres-adapter.js';
import { spacetimeAdapter } from './backends/spacetime-adapter.js';
import { stubAdapter } from './backends/stub-adapter.js';
import { convexAdapter } from './backends/convex-adapter.js';

export { leasedDatabaseEnvironment } from './stack-adapter-common.js';

export const STACK_ADAPTER_REGISTRY = createStackAdapterRegistry([
  spacetimeAdapter,
  postgresAdapter,
  mongodbAdapter,
  convexAdapter,
  stubAdapter,
]);
