import { stackIdentity } from './stack-identities.js';
import type { StackLeaseCapability, StackLeaseValidationInput } from './stack-lease-helpers.js';

// Each stack declares its lease capability in its identity module.
export function stackLeaseOperations(backend: string): StackLeaseCapability {
  return stackIdentity(backend).lease;
}

export function validateStackLeaseResources(backend: string, input: StackLeaseValidationInput): void {
  stackIdentity(backend).lease.validateResources(input);
}
