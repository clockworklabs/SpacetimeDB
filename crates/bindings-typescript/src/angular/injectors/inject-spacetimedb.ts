import { assertInInjectionContext, inject } from '@angular/core';
import type { DbConnectionImpl } from '../../sdk';
import { SPACETIMEDB_TOKEN } from '../token';

export function injectSpacetimeDB<T extends DbConnectionImpl<any>>(): T {
  assertInInjectionContext(injectSpacetimeDB);
  const spacetimedb = inject(SPACETIMEDB_TOKEN);
  return spacetimedb as T;
}
