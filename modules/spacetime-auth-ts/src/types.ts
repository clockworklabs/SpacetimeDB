import type { Infer } from 'spacetimedb/server';
import type {
  authAccountTable,
  authConfigTable,
  authSessionTable,
  authUserTable,
} from './tables.js';

export type AuthUser = Infer<typeof authUserTable.rowType>;
export type AuthSession = Infer<typeof authSessionTable.rowType>;
export type AuthAccount = Infer<typeof authAccountTable.rowType>;
export type AuthConfig = Infer<typeof authConfigTable.rowType>;
