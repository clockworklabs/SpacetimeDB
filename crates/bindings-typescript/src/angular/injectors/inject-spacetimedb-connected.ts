import { effect, signal, type Signal } from '@angular/core';
import { injectSpacetimeDB } from './inject-spacetimedb';

export function injectSpacetimeDBConnected(): Signal<boolean> {
  const conn = injectSpacetimeDB();

  const connectedSignal = signal<boolean>(conn.isActive);

  // FIXME: Bit of a dirty hack for now, we need to change injectSpacetimeDB
  // to return a signal so we can react to changes in connection state properly.
  effect(onCleanup => {
    const interval = setInterval(() => {
      connectedSignal.set(conn.isActive);
    }, 100);

    onCleanup(() => {
      clearInterval(interval);
    });
  });

  return connectedSignal.asReadonly();
}
