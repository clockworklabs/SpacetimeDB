export * from '../lib/type_builders';
export {
  schema,
  type InferSchema,
  type ModuleExport,
  type ModuleSettings,
  type SubmoduleMount,
} from './schema';
export { CaseConversionPolicy } from '../lib/autogen/types';
export { table } from '../lib/table';
export { SenderError, SpacetimeHostError, errors } from './errors';
export type { Reducer, ReducerCtx, JwtClaims, AuthCtx } from '../lib/reducers';
export type { ReducerExport } from './reducers';
export { type DbView } from './db_view';
export * from './query';
export type {
  ProcedureCtx,
  TransactionCtx,
  ProcedureExport,
} from './procedures';
export { toCamelCase } from '../lib/util';
export type { Uuid } from '../lib/uuid';
export type { Random } from './rng';
export type { ViewExport, ViewCtx, AnonymousViewCtx } from './views';
export { Range, type Bound } from './range';
export {
  Headers,
  Request,
  SyncResponse,
  Router,
  type BodyInit,
  type HeadersInit,
  type RequestInit,
  type ResponseInit,
} from './http';
export type { HandlerContext, HttpHandlerExport } from './http';
export { ScheduleAt } from '../lib/schedule_at';

export type { Environment } from './environment';

// Named so libraries built on this package can emit declaration files for
// the tables, schemas, and exports they create.
export type { CoerceRow } from '../lib/table';
export type { TableSchema } from '../lib/table_schema';
export type { TablesToSchema } from '../lib/schema';
export type { EnvironmentValue } from '../lib/environment';
export type { ReadonlyDbView } from './db_view';
export type { Schema } from './schema';

import './polyfills'; // Ensure polyfills are loaded
