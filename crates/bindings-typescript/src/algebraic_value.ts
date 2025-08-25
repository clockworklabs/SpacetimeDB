import BinaryReader from './binary_reader';

export interface ParseableType<T> {
  deserialize: (reader: BinaryReader) => T;
}

export function parseValue<T>(ty: ParseableType<T>, src: Uint8Array): T {
  const reader = new BinaryReader(src);
  return ty.deserialize(reader);
}
