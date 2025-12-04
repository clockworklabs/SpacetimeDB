export * from '../lib/type_builders';
export { schema, type InferSchema } from '../lib/schema';
export { table } from '../lib/table';
export { reducers } from '../lib/reducers';
export { SenderError, SpacetimeHostError, errors } from './errors';
export { type Reducer, type ReducerCtx } from '../lib/reducers';
export { type DbView } from './db_view';
export { and, literal } from './query';
export type { ProcedureCtx, TransactionCtx } from '../lib/procedures';

import './polyfills'; // Ensure polyfills are loaded
import './register_hooks'; // Ensure module hooks are registered
