import { describe, expect, test } from 'vitest';
import BinaryReader from '../src/lib/binary_reader.ts';
import BinaryWriter from '../src/lib/binary_writer.ts';
import { ClientMessage } from '../src/sdk/client_api/types';
import {
  countClientMessagesForV3Frame,
  decodeClientMessagesV3,
  encodeClientMessagesV3,
} from '../src/sdk/websocket_v3_frames';

function encodeClientMessage(message: ClientMessage): Uint8Array {
  const writer = new BinaryWriter(128);
  ClientMessage.serialize(writer, message);
  return writer.getBuffer().slice();
}

describe('websocket_v3_frames', () => {
  test('counts as many client messages as fit within the encoded frame limit', () => {
    const messages = [
      new Uint8Array(10),
      new Uint8Array(20),
      new Uint8Array(30),
    ];

    expect(countClientMessagesForV3Frame(messages, 10)).toBe(1);
    expect(countClientMessagesForV3Frame(messages, 30)).toBe(2);
    expect(countClientMessagesForV3Frame(messages, 60)).toBe(3);
  });

  test('still emits an oversized first message on its own', () => {
    const messages = [new Uint8Array(300_000), new Uint8Array(10)];
    expect(countClientMessagesForV3Frame(messages, 256 * 1024)).toBe(1);
  });

  test('encodes and decodes raw concatenated v2 messages', () => {
    const encodedMessages = [
      encodeClientMessage(
        ClientMessage.CallReducer({
          requestId: 7,
          flags: 0,
          reducer: 'first',
          args: new Uint8Array([1, 2]),
        })
      ),
      encodeClientMessage(
        ClientMessage.CallProcedure({
          requestId: 8,
          flags: 0,
          procedure: 'second',
          args: new Uint8Array([3, 4, 5]),
        })
      ),
    ];
    const payload = encodeClientMessagesV3(
      new BinaryWriter(128),
      encodedMessages
    );
    const decodedMessages = decodeClientMessagesV3(payload);

    expect(decodedMessages).toHaveLength(2);
    expect(
      ClientMessage.deserialize(new BinaryReader(decodedMessages[0])).tag
    ).toBe('CallReducer');
    expect(
      ClientMessage.deserialize(new BinaryReader(decodedMessages[1])).tag
    ).toBe('CallProcedure');
  });

  test('can encode only a prefix of queued client messages', () => {
    const encodedMessages = [
      encodeClientMessage(
        ClientMessage.CallReducer({
          requestId: 7,
          flags: 0,
          reducer: 'first',
          args: new Uint8Array([1, 2]),
        })
      ),
      encodeClientMessage(
        ClientMessage.CallProcedure({
          requestId: 8,
          flags: 0,
          procedure: 'second',
          args: new Uint8Array([3, 4, 5]),
        })
      ),
    ];
    const payload = encodeClientMessagesV3(
      new BinaryWriter(128),
      encodedMessages,
      1
    );
    const decodedMessages = decodeClientMessagesV3(payload);

    expect(decodedMessages).toHaveLength(1);
    expect(
      ClientMessage.deserialize(new BinaryReader(decodedMessages[0])).tag
    ).toBe('CallReducer');
  });
});
