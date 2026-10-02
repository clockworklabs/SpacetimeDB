import type { ProductType } from './algebraic_type';
import type { RawScheduleDefV10, RawTableDefV10 } from './autogen/types';
import type { IndexOpts } from './indexes';
import type { ModuleContext } from './schema';
import type { ColumnBuilder, RowBuilder } from './type_builders';
import type { HasExactlyOneKnownKey } from './type_util';
import type { ProcedureExport, ReducerExport } from '../server';

/**
 * Internal erased form of a scheduled reducer/procedure export.
 *
 * The legacy `TableOpts.scheduled` option checks the scheduled function shape
 * before it reaches `TableBody`. From here, schedule resolution only needs
 * the export object identity to look up its registered function name.
 */
export type UntypedScheduledFunctionExport =
  | ReducerExport<any, any>
  | ProcedureExport<any, any, any>;

export type TableSchedule = {
  scheduleAtCol: number;
  reducer: () => UntypedScheduledFunctionExport;
};

export type ScheduleTableForParams<Params extends Record<string, any>> =
  HasExactlyOneKnownKey<Params> extends true
    ? Params[keyof Params] extends RowBuilder<
        infer Row extends Record<string, ColumnBuilder<any, any, any>>
      >
      ? TableBody<Row, readonly IndexOpts<keyof Row & string>[]>
      : never
    : never;

/**
 * A table body: what `table()` returns. Placing it under an accessor name in
 * `schema({...})` forms a table declaration.
 */
export type TableBody<
  Row extends Record<string, ColumnBuilder<any, any, any>>,
  Idx extends readonly IndexOpts<keyof Row & string>[],
> = {
  /**
   * The name of the table.
   */
  readonly tableName?: string;

  /**
   * The TypeBuilder representation of the type of the rows in the table.
   **/
  readonly rowType: RowBuilder<Row>;

  /**
   * The {@link ProductType} representing the structure of a row in the table.
   */
  readonly rowSpacetimeType: RowBuilder<Row>['algebraicType']['value'];

  /**
   * The columns of the table, keyed by name. This is `rowType.row`.
   */
  readonly columns: RowBuilder<Row>['row'];

  /**
   * Builds the {@link RawTableDefV10} of the configured table
   */
  buildRawDef(
    ctx: ModuleContext,
    accName: string
  ): RawTableDefV10 & { schedule?: RawScheduleDefV10 };

  /**
   * Declarative multi-column indexes supplied by user code in `table({ indexes: [...] }, ...)`.
   *
   * This is intentionally the *declarative* shape (`IndexOpts`) because a lot of
   * type-level behavior is derived from these entries (for example query-builder
   * inference over composite indexes).
   */
  readonly indexes: Idx;

  /**
   * The constraints defined on the table.
   */
  readonly constraints: readonly {
    name: string | undefined;
    constraint: 'unique';
    columns: [any];
  }[];

  /**
   * The column id of the schedule-at column, if this table has a ScheduleAt column.
   */
  readonly scheduleAtCol?: number;

  /**
   * The legacy schedule defined on the table, if any.
   *
   * @deprecated Prefer `spacetime.reducer({ onSchedule: table }, ...)` or
   * `spacetime.procedure({ onSchedule: table }, ...)` so table definitions can
   * live in a separate module from reducer/procedure definitions.
   */
  readonly schedule?: TableSchedule;
};

export type UntypedTableBody = TableBody<
  Record<string, ColumnBuilder<any, any, any>>,
  readonly IndexOpts<string>[]
>;

/**
 * @deprecated Use `TableBody` instead. Kept so that declaration files emitted
 * against older versions of the SDK keep resolving.
 */
export type TableSchema<
  Row extends Record<string, ColumnBuilder<any, any, any>>,
  Idx extends readonly IndexOpts<keyof Row & string>[],
> = TableBody<Row, Idx>;

/**
 * @deprecated Use `UntypedTableBody` instead. Kept so that declaration files
 * emitted against older versions of the SDK keep resolving.
 */
export type UntypedTableSchema = UntypedTableBody;
