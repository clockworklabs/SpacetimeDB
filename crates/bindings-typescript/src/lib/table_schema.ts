import type { ProcedureExport, ReducerExport } from '../server';
import type { ProductType } from './algebraic_type';
import type { RawScheduleDefV10, RawTableDefV10 } from './autogen/types';
import type { IndexOpts } from './indexes';
import type { ModuleContext } from './schema';
import type { ColumnBuilder, Infer, RowBuilder } from './type_builders';

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
  ): Infer<typeof RawTableDefV10> & {
    schedule?: Infer<typeof RawScheduleDefV10>;
  };

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
   * The schedule defined on the table, if any.
   */
  readonly schedule?: {
    scheduleAtCol: number;
    reducer: () => ReducerExport<any, any> | ProcedureExport<any, any, any>;
  };
};

export type UntypedTableSchema = TableSchema<
  Record<string, ColumnBuilder<any, any, any>>,
  readonly IndexOpts<string>[]
>;
