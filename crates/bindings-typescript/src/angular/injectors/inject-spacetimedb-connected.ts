import {
  assertInInjectionContext,
  inject,
  computed,
  type Signal,
} from '@angular/core';
import { SPACETIMEDB_CONNECTION } from '../connection_state';

export function injectSpacetimeDBConnected(): Signal<boolean> {
  assertInInjectionContext(injectSpacetimeDBConnected);
  const state = inject(SPACETIMEDB_CONNECTION);
  return computed(() => state().isActive);
}
