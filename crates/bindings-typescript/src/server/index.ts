export * from '../lib/type_builders.ts';
export {
  schema,
  type InferSchema,
  type ModuleExport,
  type ModuleSettings,
  type SubmoduleMount,
} from './schema.ts';
export { CaseConversionPolicy } from '../lib/autogen/types.ts';
export { table } from '../lib/table.ts';
export { SenderError, SpacetimeHostError, errors } from './errors.ts';
export type {
  Reducer,
  ReducerCtx,
  JwtClaims,
  AuthCtx,
} from '../lib/reducers.ts';
export type { ReducerExport } from './reducers.ts';
export { type DbView } from './db_view.ts';
export * from './query.ts';
export type {
  ProcedureCtx,
  TransactionCtx,
  ProcedureExport,
} from './procedures.ts';
export { toCamelCase } from '../lib/util.ts';
export type { Uuid } from '../lib/uuid.ts';
export type { Random } from './rng.ts';
export type { ViewExport, ViewCtx, AnonymousViewCtx } from './views.ts';
export { Range, type Bound } from './range.ts';
export {
  Headers,
  Request,
  SyncResponse,
  Router,
  type BodyInit,
  type HeadersInit,
  type RequestInit,
  type ResponseInit,
} from './http.ts';
export type { HandlerContext, HttpHandlerExport } from './http.ts';
export { ScheduleAt } from '../lib/schedule_at.ts';

export type { Environment } from './environment.ts';

// Named so libraries built on this package can emit declaration files for
// the tables, schemas, and exports they create.
export type { CoerceRow } from '../lib/table.ts';
export type { TableSchema } from '../lib/table_schema.ts';
export type { TablesToSchema } from '../lib/schema.ts';
export type { EnvironmentValue } from '../lib/environment.ts';
export type { ReadonlyDbView } from './db_view.ts';
export type { Schema } from './schema.ts';

import './polyfills.ts'; // Ensure polyfills are loaded
