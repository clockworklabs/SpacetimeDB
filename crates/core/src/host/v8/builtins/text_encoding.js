// @ts-check

/// <reference path="types.d.ts" />

import {
  generic_decode,
  normalize_label,
  utf8_decode,
  utf8_encode,
} from 'spacetime:internal_builtins';

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

  /** @type {boolean} */
  #ignoreBOM;

  /** @type {boolean} */
  #utf8FastPath;

  /**
   * @argument {string} label
   * @argument {any} options
   */
  constructor(label = 'utf-8', options = {}) {
    if (label === 'utf-8' || label === 'utf8') {
      label = 'utf-8';
    } else {
      const normalized = normalize_label(label);
      if (normalized == null)
        throw new RangeError('The encoding label provided is invalid');
      label = normalized;
    }
    this.#encoding = label;
    this.#fatal = !!options.fatal;
    this.#ignoreBOM = !!options.ignoreBOM;
    this.#utf8FastPath = label === 'utf-8' && !this.#fatal;
  }

  get encoding() {
    return this.#encoding;
  }
  get fatal() {
    return this.#fatal;
  }
  get ignoreBOM() {
    return this.#ignoreBOM;
  }

  /**
   * @argument {AllowSharedBufferSource} input
   * @argument {any} options
   */
  decode(input, options = {}) {
    if (options.stream) {
      throw new TypeError("Option 'stream' not supported");
    }
    if (input === undefined) return '';
    if (this.#utf8FastPath) {
      return utf8_decode(input, this.#ignoreBOM);
    }
    return generic_decode(this.#encoding, input, this.#fatal, this.#ignoreBOM);
  }
};
