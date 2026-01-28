import { inject } from 'vue';
import {
  SPACETIMEDB_INJECTION_KEY,
  type ConnectionState,
} from './connection_state';

export function useSpacetimeDB(): ConnectionState {
  const context = inject(SPACETIMEDB_INJECTION_KEY);

  if (!context) {
    throw new Error(
      'useSpacetimeDB must be used within a SpacetimeDBProvider component. ' +
        'Did you forget to add a `SpacetimeDBProvider` to your component tree?'
    );
  }

  return context;
}
