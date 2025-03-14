import BinaryReader from './binary_reader';
import BinaryWriter from './binary_writer';

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
  for (let key of keys1) {
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
  let matches = str.match(/.{1,2}/g) || [];
  let data = Uint8Array.from(matches.map((byte: string) => parseInt(byte, 16)));
  if (data.length != 32) {
    return new Uint8Array(0);
  }
  return data.reverse();
}

export function hexStringToU128(str: string): bigint {
  return uint8ArrayToU128(hexStringToUint8Array(str));
}

export function hexStringToU256(str: string): bigint {
  return uint8ArrayToU256(hexStringToUint8Array(str));
}

export function u128ToUint8Array(data: bigint): Uint8Array {
  let writer = new BinaryWriter(16);
  writer.writeU128(data);
  return writer.getBuffer();
}

export function u128ToHexString(data: bigint): string {
  return uint8ArrayToHexString(u128ToUint8Array(data));
}

export function u256ToUint8Array(data: bigint): Uint8Array {
  let writer = new BinaryWriter(32);
  writer.writeU256(data);
  return writer.getBuffer();
}

export function u256ToHexString(data: bigint): string {
  return uint8ArrayToHexString(u256ToUint8Array(data));
}
