import {
  AlgebraicType,
  ProductType,
  SumType,
  type AlgebraicTypeType,
  type AlgebraicTypeVariants,
} from './algebraic_type';
import type RawModuleDefV9 from './autogen/raw_module_def_v_9_type';
import type RawScopedTypeNameV9 from './autogen/raw_scoped_type_name_v_9_type';
import type { UntypedIndex } from './indexes';
import type { UntypedTableDef } from './table';
import type { UntypedTableSchema } from './table_schema';
import {
  ArrayBuilder,
  OptionBuilder,
  ProductBuilder,
  RefBuilder,
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
import type { CamelCase } from './type_util';
import { toCamelCase } from './util';

export type TableNamesOf<S extends UntypedSchemaDef> =
  S['tables'][number]['name'];

/**
 * An untyped representation of the database schema.
 */
export type UntypedSchemaDef = {
  tables: readonly UntypedTableDef[];
};

/**
 * Helper type to convert an array of TableSchema into a schema definition
 */
export type TablesToSchema<T extends readonly UntypedTableSchema[]> = {
  tables: {
    readonly [i in keyof T]: TableToSchema<T[i]>;
  };
};

interface TableToSchema<T extends UntypedTableSchema> extends UntypedTableDef {
  name: T['tableName'];
  accessorName: CamelCase<T['tableName']>;
  columns: T['rowType']['row'];
  rowType: T['rowSpacetimeType'];
  indexes: T['idxs'];
  constraints: T['constraints'];
}

export function tablesToSchema<const T extends readonly UntypedTableSchema[]>(
  tables: T
): TablesToSchema<T> {
  return { tables: tables.map(tableToSchema) as TablesToSchema<T>['tables'] };
}

function tableToSchema<T extends UntypedTableSchema>(
  schema: T
): TableToSchema<T> {
  const getColName = (i: number) =>
    schema.rowType.algebraicType.value.elements[i].name;

  type AllowedCol = keyof T['rowType']['row'] & string;
  return {
    name: schema.tableName,
    accessorName: toCamelCase(schema.tableName as T['tableName']),
    columns: schema.rowType.row, // typed as T[i]['rowType']['row'] under TablesToSchema<T>
    rowType: schema.rowSpacetimeType,
    constraints: schema.tableDef.constraints.map(c => ({
      name: c.name,
      constraint: 'unique',
      columns: c.data.value.columns.map(getColName) as [string],
    })),
    // TODO: horrible horrible horrible. we smuggle this `Array<UntypedIndex>`
    // by casting it to an `Array<IndexOpts>` as `TableToSchema` expects.
    // This is then used in `TableCacheImpl.constructor` and who knows where else.
    // We should stop lying about our types.
    indexes: schema.tableDef.indexes.map((idx): UntypedIndex<AllowedCol> => {
      const columnIds =
        idx.algorithm.tag === 'Direct'
          ? [idx.algorithm.value]
          : idx.algorithm.value;
      return {
        name: idx.accessorName!,
        unique: schema.tableDef.constraints.some(c =>
          c.data.value.columns.every(col => columnIds.includes(col))
        ),
        algorithm: idx.algorithm.tag.toLowerCase() as 'btree',
        columns: columnIds.map(getColName),
      };
    }) as T['idxs'],
  };
}

type CompoundTypeCache = Map<
  AlgebraicTypeVariants.Product | AlgebraicTypeVariants.Sum,
  RefBuilder<any, any>
>;

type ModuleDef = Infer<typeof RawModuleDefV9>;

export class ModuleContext {
  #compoundTypes: CompoundTypeCache = new Map();
  /**
   * The global module definition that gets populated by calls to `reducer()` and lifecycle hooks.
   */
  #moduleDef: ModuleDef = {
    typespace: { types: [] },
    tables: [],
    reducers: [],
    types: [],
    miscExports: [],
    rowLevelSecurity: [],
  };

  get moduleDef() {
    return this.#moduleDef;
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
      name: splitName(name),
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

export function splitName(name: string): Infer<typeof RawScopedTypeNameV9> {
  const scope = name.split('.');
  return { name: scope.pop()!, scope };
}
