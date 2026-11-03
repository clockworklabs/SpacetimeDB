import type { ReducerCtx, InferSchema } from 'spacetimedb/server';
import type spacetimedb from './index';

export type Schema = InferSchema<typeof spacetimedb>;
export type Tx = ReducerCtx<Schema>;
export type Db = Tx['db'];
