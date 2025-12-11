import type RawTableDefV9 from './autogen/raw_table_def_v_9_type';
import type { IndexOpts } from './indexes';
import type { ModuleContext } from './schema';
import type { ColumnBuilder, Infer, RowBuilder } from './type_builders';

/**
 * Represents a handle to a database table, including its name, row type, and row spacetime type.
 */
export type TableSchema<
  TableName extends string,
  Row extends Record<string, ColumnBuilder<any, any, any>>,
  Idx extends readonly IndexOpts<keyof Row & string>[],
> = {
  /**
   * The name of the table.
   */
  readonly tableName: TableName;

  /**
   * The TypeBuilder representation of the type of the rows in the table.
   **/
  readonly rowType: RowBuilder<Row>;

  /**
   * The {@link ProductType} representing the structure of a row in the table.
   */
  readonly rowSpacetimeType: RowBuilder<Row>['algebraicType']['value'];

  /**
   * The {@link RawTableDefV9} of the configured table
   */
  tableDef(ctx: ModuleContext): Infer<typeof RawTableDefV9>;

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
};

export type UntypedTableSchema = TableSchema<
  string,
  Record<string, ColumnBuilder<any, any, any>>,
  readonly IndexOpts<string>[]
>;
