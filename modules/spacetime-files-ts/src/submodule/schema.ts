import {
  schema,
  table,
  type HandlerContext,
  type InferSchema,
  type ReducerCtx,
} from 'spacetimedb/server';
import { fileBlobRow, fileRow } from '../rows.js';

const fileOptions = {
  name: 'file',
  public: false,
  indexes: [
    {
      accessor: 'ownerPath',
      algorithm: 'btree',
      columns: ['ownerUserId', 'path'] as const,
    } as const,
  ],
};
export const file: ReturnType<
  typeof table<typeof fileRow, typeof fileOptions>
> = table(fileOptions, fileRow);

const fileBlobOptions = { name: 'file_blob', public: false };
export const fileBlob: ReturnType<
  typeof table<typeof fileBlobRow, typeof fileBlobOptions>
> = table(fileBlobOptions, fileBlobRow);

const tables: { file: typeof file; fileBlob: typeof fileBlob } = {
  file,
  fileBlob,
};
const spacetimedb: ReturnType<typeof schema<typeof tables>> = schema(tables);
export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
/** `ctx.as.files` in a host reducer, or `tx.as.files` inside a procedure's `withTx`. */
export type FilesCtx = ReducerCtx<Schema>;
/** `ctx.as.files` in a host HTTP handler. */
export type FilesHandlerCtx = HandlerContext<Schema>;
