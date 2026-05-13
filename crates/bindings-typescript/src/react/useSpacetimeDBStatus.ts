import { useContext } from 'react';
import {
  SpacetimeDBMultiContext,
  type ManagedConnectionStateMap,
} from './SpacetimeDBMultiProvider';

/**
 * Read the live state of every connection registered in the nearest
 * `<SpacetimeDBMultiProvider>`. Useful for connection-health UI.
 *
 * Returns a live `Map<label, ConnectionState>`. Throws if there is no
 * multi-provider in the component tree.
 */
export function useSpacetimeDBStatus(): ManagedConnectionStateMap {
  const map = useContext(SpacetimeDBMultiContext);
  if (!map) {
    throw new Error(
      'useSpacetimeDBStatus must be used within a SpacetimeDBMultiProvider component. Did you forget to add a `SpacetimeDBMultiProvider` to your component tree?'
    );
  }
  return map;
}
