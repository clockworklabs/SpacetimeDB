import {
  schema,
  table,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
  type TransactionCtx,
  type ViewCtx,
} from 'spacetimedb/server';
import { fileBlobRow, fileRow } from '../rows.ts';

export const file = table(
  {
    name: 'file',
    public: false,
    indexes: [
      {
        accessor: 'ownerPath',
        algorithm: 'btree',
        columns: ['ownerUserId', 'path'] as const,
      },
    ] as const,
  },
  fileRow
);

export const fileBlob = table(
  { name: 'file_blob', public: false },
  fileBlobRow
);

export const spacetimedb = schema({
  file,
  fileBlob,
});
export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
export type ReducerModuleCtx = ReducerCtx<Schema>;
export type ProcedureModuleCtx = ProcedureCtx<Schema>;
export type TransactionModuleCtx = TransactionCtx<Schema>;
export type ViewModuleCtx = ViewCtx<Schema>;
