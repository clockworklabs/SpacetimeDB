import BinaryReader from './binary_reader';
import BinaryWriter from './binary_writer';
import type { CamelCase, SnakeCase } from './type_util';

/**
 * Converts a string to PascalCase (UpperCamelCase).
 * @param str The string to convert
 * @returns The converted string
 */
export function toPascalCase(s: string): string {
  const str = s.replace(/([-_][a-z])/gi, $1 => {
    return $1.toUpperCase().replace('-', '').replace('_', '');
  });

  return str.charAt(0).toUpperCase() + str.slice(1);
}

export function deepEqual(obj1: any, obj2: any): boolean {
  // If both are strictly equal (covers primitives and reference equality), return true
  if (obj1 === obj2) return true;

  // If either is a primitive type or one is null, return false since we already checked for strict equality
  if (
    typeof obj1 !== 'object' ||
    obj1 === null ||
    typeof obj2 !== 'object' ||
    obj2 === null
  ) {
    return false;
  }

  // Get keys of both objects
  const keys1 = Object.keys(obj1);
  const keys2 = Object.keys(obj2);

  // If number of keys is different, return false
  if (keys1.length !== keys2.length) return false;

  // Check all keys and compare values recursively
  for (const key of keys1) {
    if (!keys2.includes(key) || !deepEqual(obj1[key], obj2[key])) {
      return false;
    }
  }

  return true;
}

export function uint8ArrayToHexString(array: Uint8Array): string {
  return Array.prototype.map
    .call(array.reverse(), x => ('00' + x.toString(16)).slice(-2))
    .join('');
}

export function uint8ArrayToU128(array: Uint8Array): bigint {
  if (array.length != 16) {
    throw new Error(`Uint8Array is not 16 bytes long: ${array}`);
  }
  return new BinaryReader(array).readU128();
}

export function uint8ArrayToU256(array: Uint8Array): bigint {
  if (array.length != 32) {
    throw new Error(`Uint8Array is not 32 bytes long: [${array}]`);
  }
  return new BinaryReader(array).readU256();
}

export function hexStringToUint8Array(str: string): Uint8Array {
  if (str.startsWith('0x')) {
    str = str.slice(2);
  }
  const matches = str.match(/.{1,2}/g) || [];
  const data = Uint8Array.from(
    matches.map((byte: string) => parseInt(byte, 16))
  );
  return data.reverse();
}

export function hexStringToU128(str: string): bigint {
  return uint8ArrayToU128(hexStringToUint8Array(str));
}

export function hexStringToU256(str: string): bigint {
  return uint8ArrayToU256(hexStringToUint8Array(str));
}

export function u128ToUint8Array(data: bigint): Uint8Array {
  const writer = new BinaryWriter(16);
  writer.writeU128(data);
  return writer.getBuffer();
}

export function u128ToHexString(data: bigint): string {
  return uint8ArrayToHexString(u128ToUint8Array(data));
}

export function u256ToUint8Array(data: bigint): Uint8Array {
  const writer = new BinaryWriter(32);
  writer.writeU256(data);
  return writer.getBuffer();
}

export function u256ToHexString(data: bigint): string {
  return uint8ArrayToHexString(u256ToUint8Array(data));
}

/**
 * Type safe conversion from a string like "some_identifier-name" to "someIdentifierName".
 * @param str The string to convert
 * @returns The converted string
 */
export function toCamelCase<T extends string>(str: T): CamelCase<T> {
  return str
    .replace(/[-_]+/g, '_') // collapse runs to a single separator (no backtracking issue)
    .replace(/_([a-zA-Z0-9])/g, (_, c) => c.toUpperCase()) as CamelCase<T>;
}

/** Type safe conversion from a string like "some_Identifier-name" to "some_identifier_name".
 * @param str The string to convert
 * @returns The converted string
 */
export function toSnakeCase<T extends string>(str: T): SnakeCase<T> {
  return str
    .replace(/([A-Z])/g, '_$1') // insert underscores before capitals
    .replace(/[-\s]+/g, '_') // replace spaces and dashes with underscores
    .toLowerCase() as SnakeCase<T>;
}

import type { AlgebraicType } from './algebraic_type';
import type Typespace from './autogen/typespace_type';
import type { ColumnBuilder, Infer, TypeBuilder } from './type_builders';
import type { ParamsObj } from './reducers';

export function bsatnBaseSize(
  typespace: Infer<typeof Typespace>,
  ty: AlgebraicType
): number {
  const assumedArrayLength = 4;
  while (ty.tag === 'Ref') ty = typespace.types[ty.value];
  if (ty.tag === 'Product') {
    let sum = 0;
    for (const { algebraicType: elem } of ty.value.elements) {
      sum += bsatnBaseSize(typespace, elem);
    }
    return sum;
  } else if (ty.tag === 'Sum') {
    let min = Infinity;
    for (const { algebraicType: vari } of ty.value.variants) {
      const vSize = bsatnBaseSize(typespace, vari);
      if (vSize < min) min = vSize;
    }
    if (min === Infinity) min = 0;
    return 4 + min;
  } else if (ty.tag == 'Array') {
    return 4 + assumedArrayLength * bsatnBaseSize(typespace, ty.value);
  }
  return {
    String: 4 + assumedArrayLength,
    Sum: 1,
    Bool: 1,
    I8: 1,
    U8: 1,
    I16: 2,
    U16: 2,
    I32: 4,
    U32: 4,
    F32: 4,
    I64: 8,
    U64: 8,
    F64: 8,
    I128: 16,
    U128: 16,
    I256: 32,
    U256: 32,
  }[ty.tag];
}

export type CoerceTypeBuilder<
  Col extends TypeBuilder<any, any> | ColumnBuilder<any, any, any>,
> = Col extends ColumnBuilder<any, any> ? Col['typeBuilder'] : Col;

export type CoerceParams<Params extends ParamsObj> = {
  [k in keyof Params & string]: CoerceTypeBuilder<Params[k]>;
};

export function coerceParams<Params extends ParamsObj>(
  params: Params
): CoerceParams<Params> {
  return Object.fromEntries(
    Object.entries(params).map(([n, c]) => [
      n,
      'typeBuilder' in c ? c.typeBuilder : c,
    ])
  ) as CoerceParams<Params>;
}

export const hasOwn: <K extends PropertyKey>(
  o: object,
  k: K
) => o is K extends PropertyKey ? { [k in K]: unknown } : never =
  Object.hasOwn as any;
