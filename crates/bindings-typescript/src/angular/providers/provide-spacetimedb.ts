import type { FactoryProvider } from '@angular/core';
import type { DbConnectionBuilder, DbConnectionImpl } from '../../sdk';
import { SPACETIMEDB_TOKEN } from '../token';

export function provideSpacetimeDB(
  builder: DbConnectionBuilder<DbConnectionImpl<any>>
): FactoryProvider {
  return {
    provide: SPACETIMEDB_TOKEN,
    useFactory: () => {
      const conn = builder.build();
      return conn;
    },
  };
}
