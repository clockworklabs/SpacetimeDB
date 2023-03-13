import { ProductType, SumType, AlgebraicType, BuiltinType } from './algebraic_type'

export class SumValue {
  public tag: number;
  public value: AlgebraicValue;

  constructor(tag: number, value: AlgebraicValue) {
    this.tag = tag;
    this.value = value;
  }

  public static deserialize(type: SumType | undefined, value: object): SumValue {
    if (type === undefined) {
      // TODO: get rid of undefined here
      throw "sum type is undefined";
    }

    // TODO: this will likely change, but I'm using whatever we return from the server now
    let tag = parseInt(Object.keys(value)[0]);
    let sumValue = AlgebraicValue.deserialize(type.variants[tag].algebraicType, tag);
    return new SumValue(tag, sumValue);
  }
}

export class ProductValue {
  elements: AlgebraicValue[];

  constructor(elements: AlgebraicValue[]) {
    this.elements = elements;
  }

  public static deserialize(type: ProductType | undefined, value: any): ProductValue {
    if (type === undefined) {
      throw "type is undefined"
    }

    let elements: AlgebraicValue[] = [];

    for (let i in type.elements) {
      let element = type.elements[i];
      elements.push(AlgebraicValue.deserialize(element.algebraicType, value[i]));
    }
    return new ProductValue(elements);
  }
}

type BuiltinValueType = boolean | string | number | AlgebraicValue[];

export class BuiltinValue {
  value: BuiltinValueType;

  constructor(value: BuiltinValueType) {
    this.value = value
  }

  public static deserialize(type: BuiltinType | undefined, value: any): BuiltinValue {
    if (type === undefined) {
      // TODO: what to do here? I guess I would prefer to remove this case alltogether
      return new BuiltinValue(false);
    }

    switch (type.type) {
      case BuiltinType.Type.Array:
        // TODO: handle byte array
        let result: AlgebraicValue[] = [];
        for (let el of value) {
          result.push(AlgebraicValue.deserialize(type.arrayType as AlgebraicType, el));
        }
        return new this(result);
      default:
        return new this(value);
    }
  }

  public asString(): string {
    return this.value as string;
  }
  
  public asArray(): AlgebraicValue[] {
    return this.value as AlgebraicValue[];
  }

  public asNumber(): number {
    return this.value as number;
  }
}

type AnyValue = SumValue | ProductValue | BuiltinValue;

export class AlgebraicValue {
  sum: SumValue | undefined;
  product: ProductValue | undefined;
  builtin: BuiltinValue | undefined;

  constructor(value: AnyValue | undefined) {
    if (value === undefined) {
      // TODO: possibly get rid of it
      throw "value is undefined"
    }
    switch (value.constructor) {
      case SumValue:
        this.sum = value as SumValue;
        break;
      case ProductValue:
        this.product = value as ProductValue;
        break;
      case BuiltinValue:
        this.builtin = value as BuiltinValue;
        break;
    }
  }

  public static deserialize(type: AlgebraicType, value: any) {
    switch (type.type) {
      case AlgebraicType.Type.ProductType:
        return new this(ProductValue.deserialize(type.product, value));
      case AlgebraicType.Type.SumType:
        return new this(SumValue.deserialize(type.sum, value));
      case AlgebraicType.Type.BuiltinType:
        return new this(BuiltinValue.deserialize(type.builtin, value));
      default:
        throw new Error("not implemented");
    }
  }

  public asProductValue(): ProductValue {
    if (!this.product) {
      throw "AlgebraicValue is not a ProductValue and product was requested";
    }
    return this.product as ProductValue;
  }

  public asBuiltinValue(): BuiltinValue {
    if (!this.builtin) {
      throw "AlgebraicValue is not a BuiltinValue and a builtin value was requested";
    }

    return this.builtin as BuiltinValue;
  }

  public asSumValue(): SumValue {
    if (!this.sum) {
      throw "AlgebraicValue is not a SumValue and a sum value was requested";
    }

    return this.sum as SumValue;
  }

  public asArray(): AlgebraicValue[] {
    if (!this.builtin) {
      throw "AlgebraicValue is not a BuiltinValue and an array of builtin values was requested";
    }

    return (this.builtin as BuiltinValue).asArray();
  }

  public asString(): string {
    if (!this.builtin) {
      throw "AlgebraicValue is not a BuiltinValue and a string was requested";
    }

    return (this.builtin as BuiltinValue).asString();
  }

  public asNumber(): number {
    return (this.builtin as BuiltinValue).asNumber();
  }
}


