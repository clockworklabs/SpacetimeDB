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

  public isEmpty(): boolean {
    return this.elements.length === 0;
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
    String = "String",
    Array = "Array",
    Map = "Map"
  }
}

type TypeRef = null;
type None = null;
export type EnumLabel = { label: string };

type AnyType = ProductType | SumType | BuiltinType | EnumLabel | TypeRef | None;

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

  public isProductType(): boolean {
    return this.type === Type.ProductType;
  }

  public isSumType(): boolean {
    return this.type === Type.SumType;
  }
}

export namespace AlgebraicType {
  export enum Type {
    SumType = "SumType",
    ProductType = "ProductType",
    BuiltinType = "BuiltinType",
    None = "None"
  }
}

// No idea why but in order to have a local alias for both of these
// need to be present
type Type = AlgebraicType.Type;
let Type = AlgebraicType.Type;
