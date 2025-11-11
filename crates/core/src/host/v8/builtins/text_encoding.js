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
    if (label !== 'utf-8') {
      throw new RangeError('The encoding label provided is invalid');
    }
    this.#encoding = label;
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
  decode(input, options = {}) {
    if (options.stream) {
      throw new TypeError("Option 'stream' not supported");
    }
    if (input instanceof ArrayBuffer || input instanceof SharedArrayBuffer) {
      input = new Uint8Array(input);
    }
    return utf8_decode(input, this.#fatal);
  }
};
