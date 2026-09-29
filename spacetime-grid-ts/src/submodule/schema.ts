import {
  schema,
  table,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
  type TransactionCtx,
  type ViewCtx,
} from 'spacetimedb/server';
import {
  cellStateRow,
  entityPathRow,
  gridEntityRow,
  gridRow,
} from '../rows.js';

export const grid = table({ name: 'grid', public: false }, gridRow);

const byGridCell = {
  accessor: 'byGridCell',
  algorithm: 'btree',
  columns: ['gridId', 'x', 'y'],
} as const;

export const cellState = table(
  { name: 'cell_state', public: false, indexes: [byGridCell] },
  cellStateRow
);

export const gridEntity = table(
  { name: 'grid_entity', public: false, indexes: [byGridCell] },
  gridEntityRow
);

export const entityPath = table(
  { name: 'entity_path', public: false },
  entityPathRow
);

export const spacetimedb = schema({
  grid,
  cellState,
  gridEntity,
  entityPath,
});
export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
export type ReducerModuleCtx = ReducerCtx<Schema>;
export type ProcedureModuleCtx = ProcedureCtx<Schema>;
export type TransactionModuleCtx = TransactionCtx<Schema>;
export type ViewModuleCtx = ViewCtx<Schema>;
