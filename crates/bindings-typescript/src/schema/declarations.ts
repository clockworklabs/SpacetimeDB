import type { ParamsObj } from '../lib/reducers';
import type {
  UntypedTableSchema,
  ScheduleTableForParams,
} from '../lib/table_schema';
import { type TypeBuilder } from '../lib/type_builders';
import type { CaseConversionPolicy } from '../lib/autogen/types';
import type {
  ViewReturnTypeBuilder,
  ValidateViewPrimaryKey,
} from '../server/views';

export * from '../lib/type_builders';
export { table } from '../lib/table';
export { CaseConversionPolicy } from '../lib/autogen/types';

export type ModuleDeclarations = { readonly default: Schema<any> };
export type SchemaEntry =
  | UntypedTableSchema
  | ModuleDeclarations
  | {
      readonly name: string;
      readonly module: ModuleDeclarations;
    };
export type SchemaEntries = Record<string, SchemaEntry>;
export type DeclarationKind =
  | 'reducer'
  | 'procedure'
  | 'view'
  | 'anonymousView';

export type FunctionOptions<P extends ParamsObj = ParamsObj> = {
  readonly name?: string;
  readonly onSchedule?: ScheduleTableForParams<P>;
};
export type ViewOptions = { readonly name?: string; readonly public: true };
export type SchemaOptions = {
  readonly CASE_CONVERSION_POLICY?: CaseConversionPolicy;
};

/** Metadata for a remote function. It contains no executable server callback. */
export class FunctionDeclaration<
  Kind extends DeclarationKind,
  Params extends ParamsObj,
  Return extends TypeBuilder<any, any>,
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
export class DeclarationGroup<
  E extends Record<string, FunctionDeclaration<any, any, any, any>>,
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
    const E extends Record<string, FunctionDeclaration<any, any, any, any>>,
  >(exports: E): DeclarationGroup<E> {
    return new DeclarationGroup(this, exports);
  }

  reducer<const P extends ParamsObj>(
    params: P
  ): FunctionDeclaration<'reducer', P, never, {}>;
  reducer<const P extends ParamsObj, const O extends FunctionOptions<P>>(
    options: O,
    params: P
  ): FunctionDeclaration<'reducer', P, never, O>;
  reducer(
    first: ParamsObj | FunctionOptions,
    params?: ParamsObj
  ): FunctionDeclaration<any, any, any, any> {
    return new FunctionDeclaration(
      'reducer',
      this,
      (params ?? first) as ParamsObj,
      undefined as never,
      params === undefined ? {} : first
    );
  }

  procedure<const P extends ParamsObj, R extends TypeBuilder<any, any>>(
    params: P,
    returnType: R
  ): FunctionDeclaration<'procedure', P, R, {}>;
  procedure<R extends TypeBuilder<any, any>>(
    returnType: R
  ): FunctionDeclaration<'procedure', {}, R, {}>;
  procedure<
    const P extends ParamsObj,
    R extends TypeBuilder<any, any>,
    const O extends FunctionOptions<P>,
  >(
    options: O,
    params: P,
    returnType: R
  ): FunctionDeclaration<'procedure', P, R, O>;
  procedure(...args: any[]): FunctionDeclaration<any, any, any, any> {
    const [options, params, ret] =
      args.length === 3
        ? args
        : args.length === 2
          ? [{}, ...args]
          : [{}, {}, args[0]];
    return new FunctionDeclaration('procedure', this, params, ret, options);
  }

  view<const O extends ViewOptions, R extends ViewReturnTypeBuilder>(
    options: O,
    returnType: R,
    ..._: ValidateViewPrimaryKey<R>
  ): FunctionDeclaration<'view', {}, R, O>;
  view<
    const O extends ViewOptions,
    const P extends ParamsObj,
    R extends ViewReturnTypeBuilder,
  >(
    options: O,
    params: P,
    returnType: R,
    ..._: ValidateViewPrimaryKey<R>
  ): FunctionDeclaration<'view', P, R, O>;
  view(
    options: ViewOptions,
    paramsOrReturn: ParamsObj | TypeBuilder<any, any>,
    ret?: TypeBuilder<any, any>
  ): FunctionDeclaration<any, any, any, any> {
    return new FunctionDeclaration(
      'view',
      this,
      (ret === undefined ? {} : paramsOrReturn) as ParamsObj,
      (ret ?? paramsOrReturn) as TypeBuilder<any, any>,
      options
    );
  }

  anonymousView<const O extends ViewOptions, R extends ViewReturnTypeBuilder>(
    options: O,
    returnType: R,
    ..._: ValidateViewPrimaryKey<R>
  ): FunctionDeclaration<'anonymousView', {}, R, O>;
  anonymousView<
    const O extends ViewOptions,
    const P extends ParamsObj,
    R extends TypeBuilder<any, any>,
  >(
    options: O,
    params: P,
    returnType: R,
    ..._: ValidateViewPrimaryKey<R>
  ): FunctionDeclaration<'anonymousView', P, R, O>;
  anonymousView(
    options: ViewOptions,
    paramsOrReturn: ParamsObj | TypeBuilder<any, any>,
    ret?: TypeBuilder<any, any>
  ): FunctionDeclaration<any, any, any, any> {
    return new FunctionDeclaration(
      'anonymousView',
      this,
      (ret === undefined ? {} : paramsOrReturn) as ParamsObj,
      (ret ?? paramsOrReturn) as TypeBuilder<any, any>,
      options
    );
  }
}

export function schema<const E extends SchemaEntries>(
  entries: E,
  options?: SchemaOptions
): Schema<E> {
  return new Schema(entries, options);
}
