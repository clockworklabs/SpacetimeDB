import { describe, expect, test } from 'vitest';
import { countClientMessagesForV3Frame } from '../src/sdk/websocket_v3_frames';

describe('websocket_v3_frames', () => {
  test('counts as many client messages as fit within the encoded frame limit', () => {
    const messages = [
      new Uint8Array(10),
      new Uint8Array(20),
      new Uint8Array(30),
    ];

    expect(countClientMessagesForV3Frame(messages, 1 + 4 + 10)).toBe(1);
    expect(countClientMessagesForV3Frame(messages, 1 + 4 + 4 + 10 + 4 + 20)).toBe(
      2
    );
    expect(
      countClientMessagesForV3Frame(
        messages,
        1 + 4 + 4 + 10 + 4 + 20 + 4 + 30
      )
    ).toBe(3);
  });

  test('still emits an oversized first message on its own', () => {
    const messages = [new Uint8Array(300_000), new Uint8Array(10)];
    expect(countClientMessagesForV3Frame(messages, 256 * 1024)).toBe(1);
  });
});
