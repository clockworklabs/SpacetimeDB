import { AlgebraicType, ProductType, ProductTypeElement, BuiltinType } from '../src/algebraic_type';
import { ProductValue, AlgebraicValue, SumValue, BuiltinValue } from '../src/algebraic_value';

describe('AlgebraicValue', () => {
  test('when created with a ProductValue it assigns the product property', () => {
    let value = new ProductValue([]);
    let av = new AlgebraicValue(value);

    expect(av.product).toBe(value);
    expect(av.asProductValue()).toBe(value);
  });

  test('when created with a SumValue it assigns the sum property', () => {
    let value = new SumValue(1, new AlgebraicValue(new BuiltinValue(1)));
    let av = new AlgebraicValue(value);

    expect(av.sum).toBe(value);
    expect(av.asSumValue()).toBe(value);
  });

  test('when created with a BuiltinValue it assigns the builtin property', () => {
    let value = new BuiltinValue(1);
    let av = new AlgebraicValue(value);

    expect(av.builtin).toBe(value);
    expect(av.asBuiltinValue()).toBe(value);
  });

  test('when created with a BuiltinValue(string) it can be requested as a string', () => {
    let value = new BuiltinValue("foo");
    let av = new AlgebraicValue(value);

    expect(av.asString()).toBe("foo");
  });

  test('when created with a BuiltinValue(AlgebraicValue[]) it can be requested as an array', () => {
    let array: AlgebraicValue[] = [new AlgebraicValue(new BuiltinValue(1))];
    let value = new BuiltinValue(array);
    let av = new AlgebraicValue(value);

    expect(av.asArray()).toBe(array);
  });
});

describe('ProductValue', () => {
  test('can be deserialized from an array', () => {
    let elements: AlgebraicValue[] = [new AlgebraicValue(new BuiltinValue(1))];
    let type = new ProductType([
      new ProductTypeElement("age", AlgebraicType.createPrimitiveType(BuiltinType.Type.U16))
    ]);
    let value = ProductValue.deserialize(type, [1]);

    expect(value.elements[0].asNumber()).toBe(1);
  });
});
