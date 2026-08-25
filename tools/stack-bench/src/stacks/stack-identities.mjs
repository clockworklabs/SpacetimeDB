import { MONGODB_ADAPTER_VERSION } from './backends/mongodb-identity.mjs';
import { POSTGRES_ADAPTER_VERSION } from './backends/postgres-identity.mjs';
import { SPACETIME_ADAPTER_VERSION } from './backends/spacetime-identity.mjs';
import { STUB_ADAPTER_VERSION } from './backends/stub-identity.mjs';

const VERSIONS = new Map([
  ['mongodb', MONGODB_ADAPTER_VERSION],
  ['postgres', POSTGRES_ADAPTER_VERSION],
  ['spacetime', SPACETIME_ADAPTER_VERSION],
  ['stub', STUB_ADAPTER_VERSION],
]);

export function stackAdapterVersion(id) {
  const version = VERSIONS.get(id);
  if (!version) throw new Error(`unknown stack adapter ${JSON.stringify(id)}`);
  return version;
}
