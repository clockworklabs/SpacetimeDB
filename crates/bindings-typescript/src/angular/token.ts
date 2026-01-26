import { InjectionToken } from '@angular/core';
import type { DbConnectionImpl } from '../sdk';

export const SPACETIMEDB_TOKEN = new InjectionToken<DbConnectionImpl<any>>(
  'SpacetimeDB'
);
