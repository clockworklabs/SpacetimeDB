import {
  schema,
  table,
  type HandlerContext,
  type InferSchema,
  type ReducerCtx,
} from 'spacetimedb/server';
import { fileBlobRow, fileRow } from '../rows';

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

const spacetimedb = schema({
  file,
  fileBlob,
});
export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
/** `ctx.as.files` in a host reducer, or `tx.as.files` inside a procedure's `withTx`. */
export type FilesCtx = ReducerCtx<Schema>;
/** `ctx.as.files` in a host HTTP handler. */
export type FilesHandlerCtx = HandlerContext<Schema>;
