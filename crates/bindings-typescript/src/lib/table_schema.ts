import type { ProductType } from './algebraic_type';
import type { RawScheduleDefV10, RawTableDefV10 } from './autogen/types';
import type { IndexOpts } from './indexes';
import type { ModuleContext } from './schema';
import type { ColumnBuilder, RowBuilder } from './type_builders';
import type { ProcedureExport, ReducerExport } from '../server';

/**
 * Internal erased form of a scheduled reducer/procedure export.
 *
 * The legacy `TableOpts.scheduled` option checks the scheduled function shape
 * before it reaches `TableSchema`. From here, schedule resolution only needs
 * the export object identity to look up its registered function name.
 */
export type UntypedScheduledFunctionExport =
  | ReducerExport<any, any>
  | ProcedureExport<any, any, any>;

/**
 * Represents a handle to a database table, including its name, row type, and row spacetime type.
 */
export type TableSchema<
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
   * The {@link RawTableDefV10} of the configured table
   */
  tableDef(
    ctx: ModuleContext,
    accName: string
  ): RawTableDefV10 & { schedule?: RawScheduleDefV10 };

  /**
   * The indexes defined on the table.
   */
  readonly idxs: Idx;

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
   * @deprecated Prefer `spacetime.schedule(table, reducerOrProcedure)` so table
   * definitions can live in a separate module from reducer/procedure definitions.
   */
  readonly schedule?: {
    reducer: () => UntypedScheduledFunctionExport;
  };
};

export type UntypedTableSchema = TableSchema<
  Record<string, ColumnBuilder<any, any, any>>,
  readonly IndexOpts<string>[]
>;
