import { describe, expect, test } from 'vitest';
import { BinaryReader, BinaryWriter } from '../src';
import { HttpResponse } from '../src/lib/http_types';

describe('HttpResponse header round-trip', () => {
  test('headers survive BSATN serialize/deserialize', () => {
    const textEncoder = new TextEncoder();
    const textDecoder = new TextDecoder('utf-8');

    const original: HttpResponse = {
      headers: {
        entries: [
          { name: 'content-type', value: textEncoder.encode('text/event-stream') },
          { name: 'x-request-id', value: textEncoder.encode('abc-123') },
        ],
      },
      version: { tag: 'Http11' },
      code: 200,
    };

    const writer = new BinaryWriter(256);
    HttpResponse.serialize(writer, original);
    const buf = writer.getBuffer();

    const deserialized = HttpResponse.deserialize(new BinaryReader(buf));

    expect(deserialized.code).toBe(200);
    expect(deserialized.headers.entries).toHaveLength(2);

    expect(deserialized.headers.entries[0].name).toBe('content-type');
    expect(textDecoder.decode(deserialized.headers.entries[0].value)).toBe('text/event-stream');

    expect(deserialized.headers.entries[1].name).toBe('x-request-id');
    expect(textDecoder.decode(deserialized.headers.entries[1].value)).toBe('abc-123');
  });

  test('empty headers round-trip correctly', () => {
    const original: HttpResponse = {
      headers: { entries: [] },
      version: { tag: 'Http11' },
      code: 404,
    };

    const writer = new BinaryWriter(64);
    HttpResponse.serialize(writer, original);
    const deserialized = HttpResponse.deserialize(new BinaryReader(writer.getBuffer()));

    expect(deserialized.code).toBe(404);
    expect(deserialized.headers.entries).toHaveLength(0);
  });
});
