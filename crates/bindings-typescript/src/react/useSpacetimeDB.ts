import { createContext, useContext } from 'react';
import type { ConnectionState } from './connection_state';

export const SpacetimeDBContext = createContext<ConnectionState | undefined>(
  undefined
);

// Throws an error if used outside of a SpacetimeDBProvider
// Error is caught by other hooks like useTable so they can provide better error messages
export function useSpacetimeDB(): ConnectionState {
  const context = useContext(SpacetimeDBContext) as ConnectionState | undefined;
  if (!context) {
    throw new Error(
      'useSpacetimeDB must be used within a SpacetimeDBProvider component. Did you forget to add a `SpacetimeDBProvider` to your component tree?'
    );
  }
  return context;
}
