declare module 'spacetime:internal_builtins' {
  export function utf8_encode(s: string): Uint8Array<ArrayBuffer>;
  export function utf8_decode(s: ArrayBufferView, fatal: boolean): string;
}
