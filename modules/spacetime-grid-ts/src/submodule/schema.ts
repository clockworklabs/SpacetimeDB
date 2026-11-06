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

const gridOptions = { name: 'grid', public: false };
export const grid: ReturnType<
  typeof table<typeof gridRow, typeof gridOptions>
> = table(gridOptions, gridRow);

const byGridCell = {
  accessor: 'byGridCell',
  algorithm: 'btree',
  columns: ['gridId', 'x', 'y'],
} as const;

const cellStateOptions = {
  name: 'cell_state',
  public: false,
  indexes: [byGridCell],
};
export const cellState: ReturnType<
  typeof table<typeof cellStateRow, typeof cellStateOptions>
> = table(cellStateOptions, cellStateRow);

const gridEntityOptions = {
  name: 'grid_entity',
  public: false,
  indexes: [byGridCell],
};
export const gridEntity: ReturnType<
  typeof table<typeof gridEntityRow, typeof gridEntityOptions>
> = table(gridEntityOptions, gridEntityRow);

const entityPathOptions = { name: 'entity_path', public: false };
export const entityPath: ReturnType<
  typeof table<typeof entityPathRow, typeof entityPathOptions>
> = table(entityPathOptions, entityPathRow);

const tables: {
  grid: typeof grid;
  cellState: typeof cellState;
  gridEntity: typeof gridEntity;
  entityPath: typeof entityPath;
} = {
  grid,
  cellState,
  gridEntity,
  entityPath,
};
export const spacetimedb: ReturnType<typeof schema<typeof tables>> =
  schema(tables);
export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
export type ReducerModuleCtx = ReducerCtx<Schema>;
export type ProcedureModuleCtx = ProcedureCtx<Schema>;
export type TransactionModuleCtx = TransactionCtx<Schema>;
export type ViewModuleCtx = ViewCtx<Schema>;
