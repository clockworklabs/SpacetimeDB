import { MONGODB_ADAPTER_VERSION } from './backends/mongodb-identity.js';
import { POSTGRES_ADAPTER_VERSION } from './backends/postgres-identity.js';
import { SPACETIME_ADAPTER_VERSION } from './backends/spacetime-identity.js';
import { STUB_ADAPTER_VERSION } from './backends/stub-identity.js';

const VERSIONS = new Map<string, string>([
  ['mongodb', MONGODB_ADAPTER_VERSION],
  ['postgres', POSTGRES_ADAPTER_VERSION],
  ['spacetime', SPACETIME_ADAPTER_VERSION],
  ['stub', STUB_ADAPTER_VERSION],
]);

export function stackAdapterVersion(id: string): string {
  const version = VERSIONS.get(id);
  if (!version) throw new Error(`unknown stack adapter ${JSON.stringify(id)}`);
  return version;
}
