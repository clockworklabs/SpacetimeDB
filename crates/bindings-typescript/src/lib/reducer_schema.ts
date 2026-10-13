import type { ProductType } from './algebraic_type';
import type { RawReducerDefV9 } from './autogen/types';
import type { ParamsObj } from './reducers';
import type { RowBuilder, RowObj } from './type_builders';
import type { CamelCase } from './type_util';

/**
 * Represents a handle to a database reducer, including its name and argument type.
 */
export type ReducerSchema<
  ReducerName extends string,
  Params extends ParamsObj | RowObj,
  AccessorName extends string = CamelCase<ReducerName>,
> = {
  /**
   * The name of the reducer.
   */
  readonly reducerName: ReducerName;

  /**
   * The key under which the reducer is exposed on the client, e.g. `ctx.reducers.<accessorName>`.
   * Defaults to the camelCase form of `reducerName`; generated bindings pass it explicitly.
   */
  readonly accessorName: AccessorName;

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
  readonly reducerDef: RawReducerDefV9;
};
