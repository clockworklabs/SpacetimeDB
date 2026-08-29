import type { StackAdapter } from './stack-adapter-contract.mjs';

export interface StackAdapterRegistry {
  readonly ids: readonly string[];
  get(id: string): StackAdapter;
}

export const STACK_ADAPTER_REGISTRY: StackAdapterRegistry;
