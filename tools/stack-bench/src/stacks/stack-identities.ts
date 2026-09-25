import type { StackIdentity, StackReleaseImage } from './stack-adapter-contract.js';
import { MONGODB_IDENTITY } from './backends/mongodb-identity.js';
import { POSTGRES_IDENTITY } from './backends/postgres-identity.js';
import { SPACETIME_IDENTITY } from './backends/spacetime-identity.js';
import { STUB_IDENTITY } from './backends/stub-identity.js';
import { CONVEX_IDENTITY } from './backends/convex-identity.js';
import { SUPABASE_IDENTITY } from './backends/supabase-identity.js';

const IDENTITIES = new Map<string, StackIdentity>([
  ['convex', CONVEX_IDENTITY],
  ['mongodb', MONGODB_IDENTITY],
  ['postgres', POSTGRES_IDENTITY],
  ['spacetime', SPACETIME_IDENTITY],
  ['stub', STUB_IDENTITY],
  ['supabase', SUPABASE_IDENTITY],
]);

export const STACK_IDS: readonly string[] = Object.freeze([...IDENTITIES.keys()]);

export function stackIdentity(id: string): StackIdentity {
  const found = IDENTITIES.get(id);
  if (!found) throw new Error(`unknown stack adapter ${JSON.stringify(id)}`);
  return found;
}

export function stackAdapterVersion(id: string): string {
  return stackIdentity(id).version;
}

export function stackApplicationInterface(id: string): string {
  return stackIdentity(id).applicationInterface;
}

// Every stack's pinned release images, in registry order.
export function stackReleaseImages(): StackReleaseImage[] {
  return [...IDENTITIES.values()].flatMap(identity => identity.releaseImages ?? []);
}
