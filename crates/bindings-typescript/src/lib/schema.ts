import {
  AlgebraicType,
  ProductType,
  SumType,
  type AlgebraicTypeType,
  type AlgebraicTypeVariants,
} from './algebraic_type';
import type {
  CaseConversionPolicy,
  RawSubmoduleV10,
  RawModuleDefV10,
  RawModuleDefV10Section,
  RawScopedTypeNameV10,
  RawTableDefV10,
} from './autogen/types';
import type { UntypedIndex } from './indexes';
import type { UntypedTableDecl } from './table';
import type { UntypedTableBody } from './table_body';
import {
  ArrayBuilder,
  OptionBuilder,
  ProductBuilder,
  RefBuilder,
  ResultBuilder,
  RowBuilder,
  SumBuilder,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  TypeBuilder,
  type ElementsObj,
  type Infer,
  type InferSpacetimeTypeOfTypeBuilder,
  type RowObj,
  type VariantsObj,
} from './type_builders';
import type { Values } from './type_util';

/**
 * Helper to get the set of table names.
 */
export type TableNamesOf<S extends UntypedSchemaDecl> = Values<
  S['tables']
>['accessorName'];

/**
 * Helper to get the table declaration with the given name.
 */
export type TableDeclOf<
  S extends UntypedSchemaDecl,
  N extends TableNamesOf<S>,
> = [S] extends [UntypedSchemaDecl]
  ? Values<S['tables']> & { accessorName: N }
  : UntypedTableDecl & { accessorName: N };

/**
 * An untyped representation of the database schema.
 */
export type UntypedSchemaDecl = {
  tables: Record<string, UntypedTableDecl>;
  namespaces?: Record<string, UntypedSchemaDecl>;
};

/**
 * @deprecated Use `UntypedSchemaDecl` instead. Kept so that declaration files
 * emitted against older versions of the SDK keep resolving.
 */
export type UntypedSchemaDef = UntypedSchemaDecl;

/**
 * Helper type to convert a record of table bodies into a schema declaration
 */
export interface SchemaDecl<T extends Record<string, UntypedTableBody>>
  extends UntypedSchemaDecl {
  tables: {
    readonly [AccName in keyof T & string]: TableDecl<AccName, T[AccName]>;
  };
}

/**
 * A table declaration: the table body `T` plus the fields that placing it in
 * `schema({...})` under the accessor `AccName` adds. `T` can itself be a
 * declaration placed again, so those fields replace its own.
 */
export type TableDecl<
  AccName extends string,
  T extends UntypedTableBody,
> = Omit<
  T,
  'sourceName' | 'accessorName' | 'resolvedIndexes' | 'rawDef' | 'isEvent'
> & {
  sourceName: string;
  accessorName: AccName;
  // Resolved runtime index metadata used by runtime consumers (e.g. TableCache).
  resolvedIndexes: readonly UntypedIndex<keyof T['rowType']['row'] & string>[];
  rawDef: RawTableDefV10;
  isEvent?: boolean;
};

/**
 * @deprecated Use `TableDecl` instead. Kept so that declaration files emitted
 * against older versions of the SDK keep resolving.
 */
export type TableToSchema<
  AccName extends string,
  T extends UntypedTableBody,
> = TableDecl<AccName, T>;

/**
 * @deprecated Use `SchemaDecl` instead. Kept so that declaration files emitted
 * against older versions of the SDK keep resolving.
 */
export type TablesToSchema<T extends Record<string, UntypedTableBody>> =
  SchemaDecl<T>;

export function tablesToSchema<
  const T extends Record<string, UntypedTableBody>,
>(ctx: ModuleContext, tables: T): SchemaDecl<T> {
  // `SchemaDecl<T>['tables']` is intentionally readonly in the public type,
  // but we need a mutable builder while materializing it from entries.
  type MutableTableDefs = {
    -readonly [AccName in keyof SchemaDecl<T>['tables']]: SchemaDecl<T>['tables'][AccName];
  };
  const tableDefs = Object.create(null) as MutableTableDefs;
  for (const [accName, schema] of Object.entries(tables) as [
    keyof T & string,
    T[keyof T & string],
  ][]) {
    tableDefs[accName] = tableToSchema(
      accName,
      schema,
      schema.buildRawDef(ctx, accName)
    ) as SchemaDecl<T>['tables'][typeof accName];
  }

  return {
    tables: tableDefs as SchemaDecl<T>['tables'],
  };
}

export function tableToSchema<
  AccName extends string,
  const T extends UntypedTableBody,
>(
  accName: AccName,
  schema: T,
  tableDef: RawTableDefV10
): TableDecl<AccName, T> {
  const getColName = (i: number) =>
    schema.rowType.algebraicType.value.elements[i].name;

  type AllowedCol = keyof T['rowType']['row'] & string;
  // Build fully-resolved runtime index metadata from the host-facing RawTableDef.
  // This is intentionally separate from `schema.indexes`, which keeps the original
  // user-declared `IndexOpts` shape for type-level inference.
  const resolvedIndexes: UntypedIndex<AllowedCol>[] = tableDef.indexes.map(
    idx => {
      const accessorName = idx.accessorName;
      if (typeof accessorName !== 'string' || accessorName.length === 0) {
        throw new TypeError(
          `Index '${idx.sourceName ?? '<unknown>'}' on table '${tableDef.sourceName}' is missing accessor name`
        );
      }

      const columnIds =
        idx.algorithm.tag === 'Direct'
          ? [idx.algorithm.value]
          : idx.algorithm.value;

      const unique = tableDef.constraints.some(
        c =>
          c.data.tag === 'Unique' &&
          c.data.value.columns.every(col => columnIds.includes(col))
      );

      const algorithm = (
        {
          BTree: 'btree',
          Hash: 'hash',
          Direct: 'direct',
        } as const
      )[idx.algorithm.tag];

      return {
        name: accessorName,
        unique,
        algorithm,
        columns: columnIds.map(getColName) as AllowedCol[],
      };
    }
  );

  return {
    ...schema,
    // For client,`schama.tableName` will always be there as canonical name.
    // For module, if explicit name is not provided via `name`, accessor name will
    // be used, it is stored as alias in database, hence works in query builder.
    sourceName: schema.tableName || accName,
    accessorName: accName,
    // Expose resolved runtime indexes separately so runtime users don't have to
    // reinterpret `indexes` with unsafe casts.
    resolvedIndexes,
    rawDef: tableDef,
    ...(tableDef.isEvent ? { isEvent: true } : {}),
  };
}

type CompoundTypeCache = Map<
  AlgebraicTypeVariants.Product | AlgebraicTypeVariants.Sum,
  RefBuilder<any, any>
>;

export type RawModuleDefSections = {
  [S in RawModuleDefV10Section as Uncapitalize<S['tag']>]: S['value'];
};

/** @deprecated Use `RawModuleDefSections` instead. */
export type ModuleDef = RawModuleDefSections;

type Section = RawModuleDefV10Section;

export class ModuleContext {
  #compoundTypes: CompoundTypeCache = new Map();

  /**
   * The global module definition that gets populated by calls to `reducer()` and lifecycle hooks.
   */
  #moduleDef: RawModuleDefSections = {
    typespace: { types: [] },
    tables: [],
    reducers: [],
    types: [],
    rowLevelSecurity: [],
    schedules: [],
    procedures: [],
    views: [],
    viewPrimaryKeys: [],
    lifeCycleReducers: [],
    httpHandlers: [],
    httpRoutes: [],
    caseConversionPolicy: { tag: 'SnakeCase' },
    explicitNames: {
      entries: [],
    },
    submodules: [],
    environment: [],
  };

  get moduleDef(): RawModuleDefSections {
    return this.#moduleDef;
  }

  rawModuleDefV10(): RawModuleDefV10 {
    const sections: Section[] = [];

    const push = <T extends Section>(s: T | undefined) => {
      if (s) sections.push(s);
    };

    const module = this.#moduleDef;

    push(module.typespace && { tag: 'Typespace', value: module.typespace });
    push(module.types && { tag: 'Types', value: module.types });
    push(module.tables && { tag: 'Tables', value: module.tables });
    push(module.reducers && { tag: 'Reducers', value: module.reducers });
    push(module.procedures && { tag: 'Procedures', value: module.procedures });
    push(module.views && { tag: 'Views', value: module.views });
    push(
      module.viewPrimaryKeys && {
        tag: 'ViewPrimaryKeys',
        value: module.viewPrimaryKeys,
      }
    );
    push(module.schedules && { tag: 'Schedules', value: module.schedules });
    push(
      module.lifeCycleReducers && {
        tag: 'LifeCycleReducers',
        value: module.lifeCycleReducers,
      }
    );
    push(
      module.httpHandlers && {
        tag: 'HttpHandlers',
        value: module.httpHandlers,
      }
    );
    push(
      module.httpRoutes && {
        tag: 'HttpRoutes',
        value: module.httpRoutes,
      }
    );
    push(
      module.rowLevelSecurity && {
        tag: 'RowLevelSecurity',
        value: module.rowLevelSecurity,
      }
    );
    push(
      module.explicitNames && {
        tag: 'ExplicitNames',
        value: module.explicitNames,
      }
    );
    push(
      module.caseConversionPolicy && {
        tag: 'CaseConversionPolicy',
        value: module.caseConversionPolicy,
      }
    );
    push(
      module.submodules && {
        tag: 'Submodules',
        value: module.submodules,
      }
    );
    push({ tag: 'Environment', value: module.environment });
    return { sections };
  }

  addSubmodule(submodule: RawSubmoduleV10) {
    this.#moduleDef.submodules.push(submodule);
  }

  /**
   * Set the case conversion policy for this module.
   * Called by the settings mechanism.
   */
  setCaseConversionPolicy(policy: CaseConversionPolicy) {
    this.#moduleDef.caseConversionPolicy = policy;
  }

  get typespace() {
    return this.#moduleDef.typespace;
  }

  /**
   * Resolves the actual type of a TypeBuilder by following its references until it reaches a non-ref type.
   * @param typespace The typespace to resolve types against.
   * @param typeBuilder The TypeBuilder to resolve.
   * @returns The resolved algebraic type.
   */
  public resolveType<AT extends AlgebraicTypeType>(
    typeBuilder: RefBuilder<any, AT>
  ): AT {
    let ty: AlgebraicType = typeBuilder.algebraicType;
    while (ty.tag === 'Ref') {
      ty = this.typespace.types[ty.value];
    }
    return ty as AT;
  }

  /**
   * Adds a type to the module definition's typespace as a `Ref` if it is a named compound type (Product or Sum).
   * Otherwise, returns the type as is.
   * @param name
   * @param ty
   * @returns
   */
  public registerTypesRecursively<T extends TypeBuilder<any, AlgebraicType>>(
    typeBuilder: T
  ): T extends SumBuilder<any> | ProductBuilder<any> | RowBuilder<any>
    ? RefBuilder<Infer<T>, InferSpacetimeTypeOfTypeBuilder<T>>
    : T {
    if (
      (typeBuilder instanceof ProductBuilder && !isUnit(typeBuilder)) ||
      typeBuilder instanceof SumBuilder ||
      typeBuilder instanceof RowBuilder
    ) {
      return this.#registerCompoundTypeRecursively(typeBuilder) as any;
    } else if (typeBuilder instanceof OptionBuilder) {
      return new OptionBuilder(
        this.registerTypesRecursively(typeBuilder.value)
      ) as any;
    } else if (typeBuilder instanceof ResultBuilder) {
      return new ResultBuilder(
        this.registerTypesRecursively(typeBuilder.ok),
        this.registerTypesRecursively(typeBuilder.err)
      ) as any;
    } else if (typeBuilder instanceof ArrayBuilder) {
      return new ArrayBuilder(
        this.registerTypesRecursively(typeBuilder.element)
      ) as any;
    } else {
      return typeBuilder as any;
    }
  }

  #registerCompoundTypeRecursively<
    T extends
      | SumBuilder<VariantsObj>
      | ProductBuilder<ElementsObj>
      | RowBuilder<RowObj>,
  >(typeBuilder: T): RefBuilder<Infer<T>, InferSpacetimeTypeOfTypeBuilder<T>> {
    const ty = typeBuilder.algebraicType;
    // NB! You must ensure that all TypeBuilder passed into this function
    // have a name. This function ensures that nested types always have a
    // name by assigning them one if they are missing it.
    const name = typeBuilder.typeName;
    if (name === undefined) {
      throw new Error(
        `Missing type name for ${typeBuilder.constructor.name ?? 'TypeBuilder'} ${JSON.stringify(typeBuilder)}`
      );
    }

    let r = this.#compoundTypes.get(ty);
    if (r != null) {
      // Already added to typespace
      return r;
    }

    // Recursively register nested compound types
    const newTy =
      typeBuilder instanceof RowBuilder || typeBuilder instanceof ProductBuilder
        ? ({
            tag: 'Product',
            value: { elements: [] },
          } as AlgebraicTypeVariants.Product)
        : ({
            tag: 'Sum',
            value: { variants: [] },
          } as AlgebraicTypeVariants.Sum);

    r = new RefBuilder(this.#moduleDef.typespace.types.length);
    this.#moduleDef.typespace.types.push(newTy);

    this.#compoundTypes.set(ty, r);

    if (typeBuilder instanceof RowBuilder) {
      for (const [name, elem] of Object.entries(typeBuilder.row)) {
        (newTy.value as ProductType).elements.push({
          name,
          algebraicType: this.registerTypesRecursively(elem.typeBuilder)
            .algebraicType,
        });
      }
    } else if (typeBuilder instanceof ProductBuilder) {
      for (const [name, elem] of Object.entries(typeBuilder.elements)) {
        (newTy.value as ProductType).elements.push({
          name,
          algebraicType: this.registerTypesRecursively(elem).algebraicType,
        });
      }
    } else if (typeBuilder instanceof SumBuilder) {
      for (const [name, variant] of Object.entries(typeBuilder.variants)) {
        (newTy.value as SumType).variants.push({
          name,
          algebraicType: this.registerTypesRecursively(variant).algebraicType,
        });
      }
    }

    this.#moduleDef.types.push({
      sourceName: splitName(name),
      ty: r.ref,
      customOrdering: true,
    });

    return r;
  }
}

function isUnit(typeBuilder: ProductBuilder<ElementsObj>): boolean {
  return (
    typeBuilder.typeName == null &&
    typeBuilder.algebraicType.value.elements.length === 0
  );
}

export function splitName(name: string): RawScopedTypeNameV10 {
  const scope = name.split('.');
  return { sourceName: scope.pop()!, scope };
}
