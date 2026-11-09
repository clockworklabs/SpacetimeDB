import { assertInInjectionContext, inject, type Signal } from '@angular/core';
import {
  SPACETIMEDB_CONNECTION,
  type ConnectionState,
} from '../connection_state.ts';

export function injectSpacetimeDB(): Signal<ConnectionState> {
  assertInInjectionContext(injectSpacetimeDB);
  return inject(SPACETIMEDB_CONNECTION).asReadonly();
}
