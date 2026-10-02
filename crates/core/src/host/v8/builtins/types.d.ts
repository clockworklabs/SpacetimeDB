declare module 'spacetime:internal_builtins' {
  export function utf8_encode(s: string): Uint8Array<ArrayBuffer>;
  export function utf8_decode(
    buf: AllowSharedBufferSource,
    ignoreBOM: boolean
  ): string;
  export function normalize_label(label: string): string | null;
  export function generic_decode(
    encoding: string,
    buf: AllowSharedBufferSource,
    fatal: boolean,
    ignoreBOM: boolean
  ): string;
}
