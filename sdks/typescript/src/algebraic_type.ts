export class SumTypeVariant {
  public name: string;
  public algebraicType: AlgebraicType;

  constructor(name: string, algebraicType: AlgebraicType) {
    this.name = name;
    this.algebraicType = algebraicType;
  }
}

export class SumType {
  public variants: SumTypeVariant[];

  constructor(variants: SumTypeVariant[]) {
    this.variants = variants;
  }
}

export class ProductTypeElement {
  public name: string;
  public algebraicType: AlgebraicType;

  constructor(name: string, algebraicType: AlgebraicType) {
    this.name = name;
    this.algebraicType = algebraicType;
  }
}

export class ProductType {
  public elements: ProductTypeElement[];

  constructor(elements: ProductTypeElement[]) {
    this.elements = elements;
  }
}

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

  constructor(type: BuiltinType.Type, arrayOrMapType: AlgebraicType | MapType | undefined) {
    this.type = type;
    if (arrayOrMapType !== undefined) {
      if (arrayOrMapType.constructor === MapType) {
        this.mapType = arrayOrMapType;
      } else if (arrayOrMapType.constructor === AlgebraicType) {
        this.arrayType = arrayOrMapType;
      }
    }
  }
}

// exporting BuiltinType as a namespace as well as a class allows to add
// export types on the namespace, so we can use BuiltinType.Type
export namespace BuiltinType {
  export enum Type {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    I128,
    U128,
    F32,
    F64,
    String,
    Array,
    Map
  }
}

type TypeRef = null;
type None = null;

type AnyType = ProductType | SumType | BuiltinType | TypeRef | None;

export class AlgebraicType {
  type!: Type;
  type_?: AnyType;

  public get product(): ProductType | undefined {
    return this.type == Type.ProductType ? this.type_ as ProductType : undefined;
  }
  public set product(value: ProductType | undefined) {
    this.type_ = value;
    this.type = value == undefined ? Type.None : Type.ProductType;
  }

  public get sum(): SumType | undefined {
    return this.type == Type.SumType ? this.type_ as SumType : undefined;
  }
  public set sum(value: SumType | undefined) {
    this.type_ = value;
    this.type = value == undefined ? Type.None : Type.SumType;
  }

  public get builtin(): BuiltinType | undefined {
    return this.type == Type.BuiltinType ? this.type_ as BuiltinType : undefined;
  }
  public set builtin(value: BuiltinType | undefined) {
    this.type_ = value;
    this.type = value == undefined ? Type.None : Type.BuiltinType;
  }

  public static createProductType(elements: ProductTypeElement[]): AlgebraicType {
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
}

export namespace AlgebraicType {
  export enum Type {
    SumType,
    ProductType,
    BuiltinType,
    None
  }
}

// No idea why but in order to have a local alias for both of these
// need to be present
type Type = AlgebraicType.Type;
let Type = AlgebraicType.Type;
