// @ts-check

/// <reference path="types.d.ts" />

import { utf8_decode, utf8_encode } from 'spacetime:internal_builtins';

globalThis.TextEncoder = class TextEncoder {
  constructor() {}

  get encoding() {
    return 'utf-8';
  }

  encode(input = '') {
    return utf8_encode(input);
  }
};

globalThis.TextDecoder = class TextDecoder {
  /** @type {string} */
  #encoding;

  /** @type {boolean} */
  #fatal;

  /**
   * @argument {string} label
   * @argument {any} options
   */
  constructor(label = 'utf-8', options = {}) {
    // Encoding labels are ASCII case-insensitive, with ASCII whitespace trimmed.
    // See https://encoding.spec.whatwg.org/#names-and-labels.
    switch (
      `${label}`.replace(/^[\t\n\f\r ]+|[\t\n\f\r ]+$/g, '').toLowerCase()
    ) {
      case 'unicode-1-1-utf-8':
      case 'unicode11utf8':
      case 'unicode20utf8':
      case 'utf-8':
      case 'utf8':
      case 'x-unicode20utf8':
        this.#encoding = 'utf-8';
        break;
      case 'csunicode':
      case 'iso-10646-ucs-2':
      case 'ucs-2':
      case 'unicode':
      case 'unicodefeff':
      case 'utf-16':
      case 'utf-16le':
        this.#encoding = 'utf-16le';
        break;
      default:
        throw new RangeError('The encoding label provided is invalid');
    }
    this.#fatal = !!options.fatal;
    if (options.ignoreBOM) {
      throw new TypeError("Option 'ignoreBOM' not supported");
    }
  }

  get encoding() {
    return this.#encoding;
  }
  get fatal() {
    return this.#fatal;
  }
  get ignoreBOM() {
    return false;
  }

  /**
   * @argument {any} input
   * @argument {any} options
   */
  decode(input = new Uint8Array(), options = {}) {
    if (options.stream) {
      throw new TypeError("Option 'stream' not supported");
    }
    if (input instanceof ArrayBuffer || input instanceof SharedArrayBuffer) {
      input = new Uint8Array(input);
    }
    if (this.#encoding === 'utf-16le') {
      return utf16le_decode(input, this.#fatal);
    }
    return utf8_decode(input, this.#fatal);
  }
};

/**
 * Non-streaming UTF-16LE decoding. Do not reinterpret the input as a Uint16Array:
 * views can start at odd offsets and the host's byte order need not be LE.
 * @argument {ArrayBufferView} input
 * @argument {boolean} fatal
 */
function utf16le_decode(input, fatal) {
  if (!ArrayBuffer.isView(input)) {
    throw new TypeError('argument is not an `ArrayBuffer` or a view on one');
  }
  const bytes = new DataView(input.buffer, input.byteOffset, input.byteLength);
  let output = '';
  const error = () => {
    if (fatal) {
      throw new TypeError('The encoded data is not valid UTF-16LE');
    }
    output += '\uFFFD';
  };
  let offset = 0;
  while (offset + 1 < bytes.byteLength) {
    const unit = bytes.getUint16(offset, true);
    offset += 2;
    // Strip only a leading BOM, on each independent decode call.
    if (offset === 2 && unit === 0xfeff) {
      continue;
    }
    if (unit >= 0xd800 && unit <= 0xdbff) {
      if (offset + 1 >= bytes.byteLength) {
        // A pending surrogate and an odd trailing byte are one EOF error.
        error();
        offset = bytes.byteLength;
        break;
      }
      const next = bytes.getUint16(offset, true);
      if (next >= 0xdc00 && next <= 0xdfff) {
        output += String.fromCharCode(unit, next);
        offset += 2;
      } else {
        // Leave the next code unit to be processed again after the error.
        error();
      }
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      error();
    } else {
      output += String.fromCharCode(unit);
    }
  }
  if (offset < bytes.byteLength) {
    error();
  }
  return output;
}
