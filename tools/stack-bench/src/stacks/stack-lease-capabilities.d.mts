import type { StackCapability } from './stack-adapter-contract.js';

export function stackLeaseCapability(backend: string): StackCapability;
export function executeStackLeaseCapability(
  backend: string,
  operation: string,
  input?: Record<string, unknown>,
): unknown;
