// Evaluated as a user module by the V8 host test, after installing the builtins.
function equal(actual, expected) {
  if (actual !== expected) {
    throw new Error(
      `Expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`
    );
  }
}

function throws(action, type) {
  try {
    action();
  } catch (error) {
    if (error instanceof type) {
      return;
    }
    throw error;
  }
  throw new Error(`Expected ${type.name}`);
}

// h3-js's generated browser bundle constructs both of these eagerly during
// module evaluation, including the unused UTF16Decoder (issue #5994).
const UTF8Decoder = new TextDecoder('utf8');
const UTF16Decoder = new TextDecoder('utf-16le');
equal(UTF8Decoder.encoding, 'utf-8');
equal(UTF16Decoder.encoding, 'utf-16le');
equal(UTF8Decoder.decode(new Uint8Array([0x48, 0xc3, 0xa9])), 'Hé');
equal(UTF16Decoder.decode(new Uint8Array([0x48, 0, 0xe9, 0])), 'Hé');

for (const [encoding, labels] of [
  [
    'utf-8',
    [
      'unicode-1-1-utf-8',
      'unicode11utf8',
      'unicode20utf8',
      'utf-8',
      'utf8',
      'x-unicode20utf8',
    ],
  ],
  [
    'utf-16le',
    [
      'csunicode',
      'iso-10646-ucs-2',
      'ucs-2',
      'unicode',
      'unicodefeff',
      'utf-16',
      'utf-16le',
    ],
  ],
]) {
  for (const label of labels) {
    equal(new TextDecoder(label).encoding, encoding);
    equal(
      new TextDecoder(` \t\n\f\r${label.toUpperCase()}\r\f\n\t `).encoding,
      encoding
    );
  }
}
equal(new TextDecoder().encoding, 'utf-8');
equal(new TextDecoder({ toString: () => 'UTF8' }).encoding, 'utf-8');
throws(() => new TextDecoder(Symbol()), TypeError);
for (const label of [
  '',
  'utf-16be',
  'latin1',
  'replacement',
  'utf_8',
  '\u00a0utf8',
  'utf8\u000b',
  'utf8\uFEFF',
]) {
  throws(() => new TextDecoder(label), RangeError);
}

for (const decoder of [UTF8Decoder, UTF16Decoder]) {
  equal(decoder.fatal, false);
  equal(decoder.ignoreBOM, false);
  equal(decoder.decode(), '');
  equal(decoder.decode(new Uint8Array()), '');
  throws(() => decoder.decode(null), TypeError);
  throws(() => decoder.decode([0x41, 0]), TypeError);
  throws(() => decoder.decode(new Uint8Array(), { stream: true }), TypeError);
  throws(
    () => new TextDecoder(decoder.encoding, { ignoreBOM: true }),
    TypeError
  );
}

// Buffer sources must decode the bytes in the view, including odd offsets.
const buffer = new Uint8Array([0xff, 0x41, 0, 0x3d, 0xd8, 0, 0xde, 0xff]);
equal(UTF16Decoder.decode(buffer.subarray(1, 7)), 'A😀');
equal(UTF16Decoder.decode(new DataView(buffer.buffer, 1, 6)), 'A😀');
equal(UTF16Decoder.decode(buffer.slice(1, 7).buffer), 'A😀');
const words = new Uint16Array(3);
new Uint8Array(words.buffer).set([0x41, 0, 0x3d, 0xd8, 0, 0xde]);
equal(UTF16Decoder.decode(words), 'A😀');
const shared = new SharedArrayBuffer(6);
new Uint8Array(shared).set([0x41, 0, 0x3d, 0xd8, 0, 0xde]);
equal(UTF16Decoder.decode(shared), 'A😀');
equal(UTF16Decoder.decode(new DataView(shared, 2, 4)), '😀');

equal(
  UTF16Decoder.decode(new Uint8Array([0xff, 0xfe, 0x41, 0, 0xff, 0xfe])),
  'A\uFEFF'
);
equal(UTF16Decoder.decode(new Uint8Array([0xff, 0xfe])), '');
equal(UTF16Decoder.decode(new Uint8Array([0xff, 0xfe, 0x42, 0])), 'B');
// A big-endian BOM does not change the selected encoding.
equal(
  UTF16Decoder.decode(new Uint8Array([0xfe, 0xff, 0, 0x41])),
  '\uFFFE\u4100'
);
equal(UTF16Decoder.decode(new Uint8Array([0, 0, 0xff, 0xff])), '\0\uFFFF');

const fatal = new TextDecoder('utf-16le', { fatal: true });
equal(fatal.fatal, true);
equal(fatal.decode(new Uint8Array([0xff, 0xfe, 0x3d, 0xd8, 0, 0xde])), '😀');
for (const [bytes, expected] of [
  [[0x41], '\uFFFD'],
  [[0x41, 0, 0x42], 'A\uFFFD'],
  [[0, 0xdc], '\uFFFD'],
  [[0, 0xd8], '\uFFFD'],
  [[0, 0xd8, 0x41], '\uFFFD'],
  [[0, 0xd8, 0x41, 0], '\uFFFDA'],
  [[0, 0xd8, 0, 0xd8, 0, 0xdc], '\uFFFD𐀀'],
  [[0, 0xdc, 0, 0xdc], '\uFFFD\uFFFD'],
  [[0xff, 0xfe, 0, 0xd8], '\uFFFD'],
]) {
  const input = new Uint8Array(bytes);
  equal(UTF16Decoder.decode(input), expected);
  throws(() => fatal.decode(input), TypeError);
  // Independent calls reset decoding after a fatal error.
  equal(fatal.decode(new Uint8Array([0x41, 0])), 'A');
}

equal(UTF8Decoder.decode(new Uint8Array([0xff])), '\uFFFD');
throws(
  () => new TextDecoder('utf8', { fatal: true }).decode(new Uint8Array([0xff])),
  TypeError
);
// Large inputs should not hit the argument-count limit of fromCharCode(...).
const large = new Uint8Array(140000);
for (let i = 0; i < large.length; i += 2) {
  large[i] = 0x41;
}
equal(UTF16Decoder.decode(large), 'A'.repeat(70000));
