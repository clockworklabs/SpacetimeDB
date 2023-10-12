/**
 * A variant of a sum type.
 *
 * NOTE: Each element has an implicit element tag based on its order.
 * Uniquely identifies an element similarly to protobuf tags.
 */
export class SumTypeVariant {
  public name: string;
  public algebraicType: AlgebraicType;

  constructor(name: string, algebraicType: AlgebraicType) {
    this.name = name;
    this.algebraicType = algebraicType;
  }
}

/**
 * Unlike most languages, sums in SATS are *[structural]* and not nominal.
 * When checking whether two nominal types are the same,
 * their names and/or declaration sites (e.g., module / namespace) are considered.
 * Meanwhile, a structural type system would only check the structure of the type itself,
 * e.g., the names of its variants and their inner data types in the case of a sum.
 *
 * This is also known as a discriminated union (implementation) or disjoint union.
 * Another name is [coproduct (category theory)](https://ncatlab.org/nlab/show/coproduct).
 *
 * These structures are known as sum types because the number of possible values a sum
 * ```ignore
 * { N_0(T_0), N_1(T_1), ..., N_n(T_n) }
 * ```
 * is:
 * ```ignore
 * Σ (i ∈ 0..n). values(T_i)
 * ```
 * so for example, `values({ A(U64), B(Bool) }) = values(U64) + values(Bool)`.
 *
 * See also: https://ncatlab.org/nlab/show/sum+type.
 *
 * [structural]: https://en.wikipedia.org/wiki/Structural_type_system
 */
export class SumType {
  public variants: SumTypeVariant[];

  constructor(variants: SumTypeVariant[]) {
    this.variants = variants;
  }
}

/**
 * A factor / element of a product type.
 *
 * An element consist of an optional name and a type.
 *
 * NOTE: Each element has an implicit element tag based on its order.
 * Uniquely identifies an element similarly to protobuf tags.
 */
export class ProductTypeElement {
  public name: string;
  public algebraicType: AlgebraicType;

  constructor(name: string, algebraicType: AlgebraicType) {
    this.name = name;
    this.algebraicType = algebraicType;
  }
}

/**
 * A structural product type  of the factors given by `elements`.
 *
 * This is also known as `struct` and `tuple` in many languages,
 * but note that unlike most languages, products in SATs are *[structural]* and not nominal.
 * When checking whether two nominal types are the same,
 * their names and/or declaration sites (e.g., module / namespace) are considered.
 * Meanwhile, a structural type system would only check the structure of the type itself,
 * e.g., the names of its fields and their types in the case of a record.
 * The name "product" comes from category theory.
 *
 * See also: https://ncatlab.org/nlab/show/product+type.
 *
 * These structures are known as product types because the number of possible values in product
 * ```ignore
 * { N_0: T_0, N_1: T_1, ..., N_n: T_n }
 * ```
 * is:
 * ```ignore
 * Π (i ∈ 0..n). values(T_i)
 * ```
 * so for example, `values({ A: U64, B: Bool }) = values(U64) * values(Bool)`.
 *
 * [structural]: https://en.wikipedia.org/wiki/Structural_type_system
 */
export class ProductType {
  public elements: ProductTypeElement[];

  constructor(elements: ProductTypeElement[]) {
    this.elements = elements;
  }

  public isEmpty(): boolean {
    return this.elements.length === 0;
  }
}

/* A map type from keys of type `keyType` to values of type `valueType`. */
export class MapType {
  public keyType: AlgebraicType;
  public valueType: AlgebraicType;

  constructor(keyType: AlgebraicType, valueType: AlgebraicType) {
    this.keyType = keyType;
    this.valueType = valueType;
  }
}

type ArrayBaseType = AlgebraicType;
type TypeRef = null;
type None = null;

type AnyType = ProductType | SumType | ArrayBaseType | MapType | TypeRef | None;

/**
 * The SpacetimeDB Algebraic Type System (SATS) is a structural type system in
 * which a nominal type system can be constructed.
 *
 * The type system unifies the concepts sum types, product types, and built-in
 * primitive types into a single type system.
 */
export class AlgebraicType {
  type!: Type;
  type_?: AnyType;

  private setter(type: Type, payload: AnyType | undefined) {
    this.type_ = payload;
    this.type = payload === undefined ? Type.None : type;
  }

  public get product(): ProductType {
    if (this.type !== Type.Product) {
      throw "product type was requested, but the type is not ProductType";
    }
    return this.type_ as ProductType;
  }
  public set product(value: ProductType | undefined) {
    this.setter(Type.Product, value);
  }

  public get sum(): SumType {
    if (this.type !== Type.Sum) {
      throw "sum type was requested, but the type is not SumType";
    }
    return this.type_ as SumType;
  }
  public set sum(value: SumType | undefined) {
    this.setter(Type.Sum, value);
  }

  public get array(): AlgebraicType {
    if (this.type !== Type.Array) {
      throw "array type was requested, but the type is not Array";
    }
    return this.type_ as AlgebraicType;
  }
  public set array(value: AlgebraicType | undefined) {
    this.setter(Type.Array, value);
  }

  public get map(): MapType {
    if (this.type !== Type.Map) {
      throw "map type was requested, but the type is not MapType";
    }
    return this.type_ as MapType;
  }
  public set map(value: MapType | undefined) {
    this.setter(Type.Map, value);
  }

  private static createType(
    type: Type,
    payload: AnyType | undefined
  ): AlgebraicType {
    let at = new AlgebraicType();
    at.setter(type, payload);
    return at;
  }

  public static createProductType(
    elements: ProductTypeElement[]
  ): AlgebraicType {
    let type = new AlgebraicType();
    type.product = new ProductType(elements);
    return type;
  }

  public static createSumType(variants: SumTypeVariant[]): AlgebraicType {
    let type = new AlgebraicType();
    type.sum = new SumType(variants);
    return type;
  }

  public static createArrayType(elementType: AlgebraicType): AlgebraicType {
    let type = new AlgebraicType();
    type.array = elementType;
    return type;
  }

  public static createMapType(
    key: AlgebraicType,
    val: AlgebraicType
  ): AlgebraicType {
    let type = new AlgebraicType();
    type.map = new MapType(key, val);
    return type;
  }

  public static createBoolType(): AlgebraicType {
    return this.createType(Type.Bool, null);
  }
  public static createI8Type(): AlgebraicType {
    return this.createType(Type.I8, null);
  }
  public static createU8Type(): AlgebraicType {
    return this.createType(Type.U8, null);
  }
  public static createI16Type(): AlgebraicType {
    return this.createType(Type.I16, null);
  }
  public static createU16Type(): AlgebraicType {
    return this.createType(Type.U16, null);
  }
  public static createI32Type(): AlgebraicType {
    return this.createType(Type.I32, null);
  }
  public static createU32Type(): AlgebraicType {
    return this.createType(Type.U32, null);
  }
  public static createI64Type(): AlgebraicType {
    return this.createType(Type.I64, null);
  }
  public static createU64Type(): AlgebraicType {
    return this.createType(Type.U64, null);
  }
  public static createI128Type(): AlgebraicType {
    return this.createType(Type.I128, null);
  }
  public static createU128Type(): AlgebraicType {
    return this.createType(Type.U128, null);
  }
  public static createF32Type(): AlgebraicType {
    return this.createType(Type.F32, null);
  }
  public static createF64Type(): AlgebraicType {
    return this.createType(Type.F64, null);
  }
  public static createStringType(): AlgebraicType {
    return this.createType(Type.String, null);
  }
  public static createBytesType(): AlgebraicType {
    return this.createArrayType(this.createU8Type());
  }

  public isProductType(): boolean {
    return this.type === Type.Product;
  }

  public isSumType(): boolean {
    return this.type === Type.Sum;
  }

  public isArrayType(): boolean {
    return this.type === Type.Array;
  }

  public isMapType(): boolean {
    return this.type === Type.Map;
  }
}

export namespace AlgebraicType {
  export enum Type {
    Sum = "SumType",
    Product = "ProductType",
    /** This is a SATS `ArrayType`
     *
     * An array type is a **homogeneous** product type of dynamic length.
     *
     * That is, it is a product type
     * where every element / factor / field is of the same type
     * and where the length is statically unknown.
     */
    Array = "Array",
    /** This is a SATS `MapType` */
    Map = "Map",
    Bool = "Bool",
    I8 = "I8",
    U8 = "U8",
    I16 = "I16",
    U16 = "U16",
    I32 = "I32",
    U32 = "U32",
    I64 = "I64",
    U64 = "U64",
    I128 = "I128",
    U128 = "U128",
    F32 = "F32",
    F64 = "F64",
    /** UTF-8 encoded */
    String = "String",
    None = "None",
  }
}

// No idea why but in order to have a local alias for both of these
// need to be present
type Type = AlgebraicType.Type;
let Type = AlgebraicType.Type;
