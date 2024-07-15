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

export class BuiltinType {
  public type: BuiltinType.Type;
  public arrayType: AlgebraicType | undefined;
  public mapType: MapType | undefined;

  constructor(
    type: BuiltinType.Type,
    arrayOrMapType: AlgebraicType | MapType | undefined
  ) {
    this.type = type;
    if (arrayOrMapType !== undefined) {
      if (arrayOrMapType.constructor === MapType) {
        this.mapType = arrayOrMapType;
      } else if (arrayOrMapType.constructor === AlgebraicType) {
        this.arrayType = arrayOrMapType;
      }
    }
  }

  public static bytes(): BuiltinType {
    return new BuiltinType(
      BuiltinType.Type.Array,
      AlgebraicType.createPrimitiveType(BuiltinType.Type.U8)
    );
  }

  public static string_ty(): BuiltinType {
    return new BuiltinType(BuiltinType.Type.String, undefined);
  }
}

// exporting BuiltinType as a namespace as well as a class allows to add
// export types on the namespace, so we can use BuiltinType.Type
/*
 * Represents the built-in types in SATS.
 *
 * Some of these types are nominal in our otherwise structural type system.
 */
export namespace BuiltinType {
  export enum Type {
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
  }
}

type TypeRef = null;
type None = null;
export type EnumLabel = { label: string };

type AnyType = ProductType | SumType | BuiltinType | EnumLabel | TypeRef | None;

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

  public get product(): ProductType {
    if (this.type !== Type.ProductType) {
      throw "product type was requested, but the type is not ProductType";
    }
    return this.type_ as ProductType;
  }

  public set product(value: ProductType | undefined) {
    this.type_ = value;
    this.type = value == undefined ? Type.None : Type.ProductType;
  }

  public get sum(): SumType {
    if (this.type !== Type.SumType) {
      throw "sum type was requested, but the type is not SumType";
    }
    return this.type_ as SumType;
  }
  public set sum(value: SumType | undefined) {
    this.type_ = value;
    this.type = value == undefined ? Type.None : Type.SumType;
  }

  public get builtin(): BuiltinType {
    if (this.type !== Type.BuiltinType) {
      throw "builtin type was requested, but the type is not BuiltinType";
    }
    return this.type_ as BuiltinType;
  }
  public set builtin(value: BuiltinType | undefined) {
    this.type_ = value;
    this.type = value == undefined ? Type.None : Type.BuiltinType;
  }

  public static createProductType(
    elements: ProductTypeElement[]
  ): AlgebraicType {
    let type = new AlgebraicType();
    type.product = new ProductType(elements);
    return type;
  }

  public static createArrayType(elementType: AlgebraicType) {
    let type = new AlgebraicType();
    type.builtin = new BuiltinType(BuiltinType.Type.Array, elementType);
    return type;
  }

  public static createSumType(variants: SumTypeVariant[]): AlgebraicType {
    let type = new AlgebraicType();
    type.sum = new SumType(variants);
    return type;
  }

  public static createPrimitiveType(type: BuiltinType.Type) {
    let algebraicType = new AlgebraicType();
    algebraicType.builtin = new BuiltinType(type, undefined);
    return algebraicType;
  }

  public isProductType(): boolean {
    return this.type === Type.ProductType;
  }

  public isSumType(): boolean {
    return this.type === Type.SumType;
  }

  public isBuiltinType(): boolean {
    return this.type === Type.BuiltinType;
  }

  private isBytes(): boolean {
    return (
      this.isBuiltinType() &&
      this.builtin.type === BuiltinType.Type.Array &&
      (this.builtin.arrayType as AlgebraicType).isBuiltinType() &&
      ((this.builtin.arrayType as AlgebraicType).builtin as BuiltinType).type ==
        BuiltinType.Type.U8
    );
  }

  private isBytesNewtype(tag: string): boolean {
    return (
      this.isProductType() &&
      this.product.elements.length === 1 &&
      this.product.elements[0].algebraicType.isBytes() &&
      this.product.elements[0].name === tag
    );
  }

  public isIdentity(): boolean {
    return this.isBytesNewtype("__identity_bytes");
  }

  public isAddress(): boolean {
    return this.isBytesNewtype("__address_bytes");
  }
}

export namespace AlgebraicType {
  export enum Type {
    SumType = "SumType",
    ProductType = "ProductType",
    BuiltinType = "BuiltinType",
    None = "None",
  }
}

// No idea why but in order to have a local alias for both of these
// need to be present
type Type = AlgebraicType.Type;
let Type = AlgebraicType.Type;
