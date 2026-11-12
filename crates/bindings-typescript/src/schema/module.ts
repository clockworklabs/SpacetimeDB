/**
 * Mirrors the module schema API in ../server/schema.ts for generated client bindings,
 * preserving schemas and metadata while omitting server function bodies.
 */
import type { ParamsObj } from '../lib/reducers.ts';
import type { UntypedTableSchema } from '../lib/table_schema.ts';
import { type TypeBuilder } from '../lib/type_builders.ts';
import type { ReducerOptsWithOptionalName } from '../server/reducers.ts';
import type { ProcedureOptsWithOptionalName } from '../server/procedures.ts';
import type { ModuleSettings } from '../server/schema.ts';
import type {
  ViewOpts,
  ViewReturnTypeBuilder,
  ValidateViewPrimaryKey,
} from '../server/views.ts';

export * from '../lib/type_builders.ts';
export { table } from '../lib/table.ts';
export { CaseConversionPolicy } from '../lib/autogen/types.ts';

export type ModuleSchemaExports = { readonly default: Schema<any> };
export type SchemaEntry =
  | UntypedTableSchema
  | ModuleSchemaExports
  | {
      readonly name: string;
      readonly module: ModuleSchemaExports;
    };
export type SchemaEntries = Record<string, SchemaEntry>;
export type FunctionKind = 'reducer' | 'procedure' | 'view' | 'anonymousView';

export type FunctionOptions<P extends ParamsObj = ParamsObj> = Readonly<
  ReducerOptsWithOptionalName<P>
>;
export type ViewOptions = Readonly<ViewOpts>;
export type SchemaOptions = Readonly<
  Pick<ModuleSettings, 'CASE_CONVERSION_POLICY'>
>;

/** Metadata for a remote function. It contains no executable server callback. */
export class FunctionSchema<
  Kind extends FunctionKind,
  Params extends ParamsObj,
  Return extends TypeBuilder<any, any> | undefined,
  Options extends object = object,
> {
  constructor(
    readonly kind: Kind,
    readonly schema: Schema<any>,
    readonly params: Params,
    readonly returnType: Return,
    readonly options: Options
  ) {}
}

/** The group's keys are function accessors, not an extra namespace. */
export class FunctionGroup<
  E extends Record<string, FunctionSchema<any, any, any, any>>,
> {
  constructor(
    readonly schema: Schema<any>,
    readonly exports: E
  ) {}
}

export class Schema<Entries extends SchemaEntries> {
  constructor(
    readonly entries: Entries,
    readonly options: SchemaOptions = {}
  ) {}

  exportGroup<
    const E extends Record<string, FunctionSchema<any, any, any, any>>,
  >(exports: E): FunctionGroup<E> {
    return new FunctionGroup(this, exports);
  }

  reducer<const P extends ParamsObj>(
    params: P
  ): FunctionSchema<'reducer', P, undefined, {}>;
  reducer<const P extends ParamsObj, const O extends FunctionOptions<P>>(
    options: O,
    params: P
  ): FunctionSchema<'reducer', P, undefined, O>;
  reducer(
    ...args: [params: ParamsObj] | [options: FunctionOptions, params: ParamsObj]
  ): FunctionSchema<any, any, any, any> {
    const [options, params] = args.length === 1 ? [{}, args[0]] : args;
    return new FunctionSchema('reducer', this, params, undefined, options);
  }

  procedure<const P extends ParamsObj, R extends TypeBuilder<any, any>>(
    params: P,
    returnType: R
  ): FunctionSchema<'procedure', P, R, {}>;
  procedure<R extends TypeBuilder<any, any>>(
    returnType: R
  ): FunctionSchema<'procedure', {}, R, {}>;
  procedure<
    const P extends ParamsObj,
    R extends TypeBuilder<any, any>,
    const O extends Readonly<ProcedureOptsWithOptionalName<P, R>>,
  >(options: O, params: P, returnType: R): FunctionSchema<'procedure', P, R, O>;
  procedure(
    ...args:
      | [returnType: TypeBuilder<any, any>]
      | [params: ParamsObj, returnType: TypeBuilder<any, any>]
      | [options: object, params: ParamsObj, returnType: TypeBuilder<any, any>]
  ): FunctionSchema<any, any, any, any> {
    const [options, params, ret] =
      args.length === 3
        ? args
        : args.length === 2
          ? [{}, ...args]
          : [{}, {}, args[0]];
    return new FunctionSchema('procedure', this, params, ret, options);
  }

  view<const O extends ViewOptions, R extends ViewReturnTypeBuilder>(
    options: O,
    returnType: R,
    ..._: ValidateViewPrimaryKey<R>
  ): FunctionSchema<'view', {}, R, O>;
  view<
    const O extends ViewOptions,
    const P extends ParamsObj,
    R extends ViewReturnTypeBuilder,
  >(
    options: O,
    params: P,
    returnType: R,
    ..._: ValidateViewPrimaryKey<R>
  ): FunctionSchema<'view', P, R, O>;
  view(
    ...args:
      | [options: ViewOptions, returnType: TypeBuilder<any, any>]
      | [
          options: ViewOptions,
          params: ParamsObj,
          returnType: TypeBuilder<any, any>,
        ]
  ): FunctionSchema<any, any, any, any> {
    if (args.length === 2)
      return new FunctionSchema('view', this, {}, args[1], args[0]);
    return new FunctionSchema('view', this, args[1], args[2], args[0]);
  }

  anonymousView<const O extends ViewOptions, R extends ViewReturnTypeBuilder>(
    options: O,
    returnType: R,
    ..._: ValidateViewPrimaryKey<R>
  ): FunctionSchema<'anonymousView', {}, R, O>;
  anonymousView<
    const O extends ViewOptions,
    const P extends ParamsObj,
    R extends TypeBuilder<any, any>,
  >(
    options: O,
    params: P,
    returnType: R,
    ..._: ValidateViewPrimaryKey<R>
  ): FunctionSchema<'anonymousView', P, R, O>;
  anonymousView(
    ...args:
      | [options: ViewOptions, returnType: TypeBuilder<any, any>]
      | [
          options: ViewOptions,
          params: ParamsObj,
          returnType: TypeBuilder<any, any>,
        ]
  ): FunctionSchema<any, any, any, any> {
    if (args.length === 2)
      return new FunctionSchema('anonymousView', this, {}, args[1], args[0]);
    return new FunctionSchema('anonymousView', this, args[1], args[2], args[0]);
  }
}

export function schema<const E extends SchemaEntries>(
  entries: E,
  options?: SchemaOptions
): Schema<E> {
  return new Schema(entries, options);
}
