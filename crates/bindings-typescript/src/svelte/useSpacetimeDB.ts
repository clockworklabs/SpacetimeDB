import { getContext } from 'svelte';
import type { Writable } from 'svelte/store';
import {
  SPACETIMEDB_CONTEXT_KEY,
  type ConnectionState,
} from './connection_state';

// Throws an error if used outside of a SpacetimeDBProvider
export function useSpacetimeDB(): Writable<ConnectionState> {
  const context = getContext<Writable<ConnectionState> | undefined>(
    SPACETIMEDB_CONTEXT_KEY
  );

  if (!context) {
    throw new Error(
      'useSpacetimeDB must be used within a component that called createSpacetimeDBProvider. ' +
        'Did you forget to call `createSpacetimeDBProvider` in a parent component?'
    );
  }

  return context;
}
