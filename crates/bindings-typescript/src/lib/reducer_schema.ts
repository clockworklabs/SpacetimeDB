import type { ProductType } from './algebraic_type';
import type RawReducerDefV9 from './autogen/raw_reducer_def_v_9_type';
import type { ParamsObj } from './reducers';
import type { Infer, RowBuilder, RowObj } from './type_builders';
import type { CamelCase } from './type_util';

/**
 * Represents a handle to a database reducer, including its name and argument type.
 */
export type ReducerSchema<
  ReducerName extends string,
  Params extends ParamsObj | RowObj,
> = {
  /**
   * The name of the reducer.
   */
  readonly reducerName: ReducerName;

  /**
   * The accessor name for the reducer.
   */
  readonly accessorName: CamelCase<ReducerName>;

  /**
   * The TypeBuilder representation of the reducer's parameter type.
   */
  readonly params: RowBuilder<Params>;

  /**
   * The {@link ProductType} representing the structure of the reducer's parameters.
   */
  readonly paramsSpacetimeType: ProductType;

  /**
   * The {@link RawReducerDefV9} of the configured reducer.
   */
  readonly reducerDef: Infer<typeof RawReducerDefV9>;
};
