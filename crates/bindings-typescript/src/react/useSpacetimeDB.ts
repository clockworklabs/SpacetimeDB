import { createContext, useContext } from 'react';
import type { ConnectionState } from './connection_state';
import { SpacetimeDBMultiContext } from './SpacetimeDBMultiProvider';

export const SpacetimeDBContext = createContext<ConnectionState | undefined>(
  undefined
);

/**
 * Read the live SpacetimeDB connection state.
 *
 * - `useSpacetimeDB()` — reads the nearest `<SpacetimeDBProvider>`. Throws if
 *   there is none.
 * - `useSpacetimeDB(key)` — reads the entry labelled `key` from the nearest
 *   `<SpacetimeDBMultiProvider>`. Throws if there is no multi-provider, or
 *   if `key` is not registered.
 */
export function useSpacetimeDB(key?: string): ConnectionState {
  // Call both context hooks unconditionally so hook order is stable across
  // keyed / un-keyed usage.
  const singleContext = useContext(SpacetimeDBContext) as
    | ConnectionState
    | undefined;
  const multiContext = useContext(SpacetimeDBMultiContext);

  if (key !== undefined) {
    if (!multiContext) {
      throw new Error(
        `useSpacetimeDB('${key}') must be used within a SpacetimeDBMultiProvider component. Did you forget to add a \`SpacetimeDBMultiProvider\` to your component tree?`
      );
    }
    const state = multiContext.get(key);
    if (!state) {
      const known = Array.from(multiContext.keys()).join(', ') || '(none)';
      throw new Error(
        `useSpacetimeDB('${key}'): no connection registered under that key. Known keys: ${known}.`
      );
    }
    return state;
  }

  if (!singleContext) {
    throw new Error(
      'useSpacetimeDB must be used within a SpacetimeDBProvider component. Did you forget to add a `SpacetimeDBProvider` to your component tree?'
    );
  }
  return singleContext;
}
