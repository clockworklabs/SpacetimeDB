import BinaryReader from '../lib/binary_reader.ts';
import BinaryWriter from '../lib/binary_writer.ts';
import { ClientMessage, ServerMessage } from './client_api/types';

// v3 is only a transport framing convention. The payload is one or more
// already-encoded v2 websocket messages concatenated back-to-back, so these
// helpers intentionally operate on raw bytes.
const EMPTY_V3_PAYLOAD_ERR =
  'v3 websocket payloads must contain at least one message';

function ensureMessages(messages: readonly Uint8Array[]): void {
  if (messages.length === 0) {
    throw new RangeError(EMPTY_V3_PAYLOAD_ERR);
  }
}

function ensureMessageCount(
  messages: readonly Uint8Array[],
  messageCount: number
): void {
  ensureMessages(messages);
  if (messageCount < 1 || messageCount > messages.length) {
    throw new RangeError(
      `v3 websocket payload requested ${messageCount} messages from ${messages.length}`
    );
  }
}

function concatenateMessagesV3(
  writer: BinaryWriter,
  messages: readonly Uint8Array<ArrayBuffer>[],
  messageCount: number = messages.length
): Uint8Array<ArrayBuffer> {
  ensureMessageCount(messages, messageCount);
  writer.clear();
  for (let i = 0; i < messageCount; i++) {
    writer.writeBytes(messages[i]!);
  }
  return writer.getBuffer();
}

function splitMessagesV3(
  reader: BinaryReader,
  data: Uint8Array<ArrayBuffer>,
  deserialize: (reader: BinaryReader) => unknown
): Uint8Array<ArrayBuffer>[] {
  reader.reset(data);
  if (reader.remaining === 0) {
    throw new RangeError(EMPTY_V3_PAYLOAD_ERR);
  }

  const messages: Uint8Array<ArrayBuffer>[] = [];
  while (reader.remaining > 0) {
    const startOffset = reader.offset;
    deserialize(reader);
    messages.push(data.subarray(startOffset, reader.offset));
  }

  return messages;
}

export function countClientMessagesForV3Frame(
  messages: readonly Uint8Array<ArrayBuffer>[],
  maxFrameBytes: number
): number {
  ensureMessages(messages);

  const firstMessage = messages[0]!;
  if (firstMessage.length > maxFrameBytes) {
    return 1;
  }

  let count = 1;
  let frameSize = firstMessage.length;
  while (count < messages.length) {
    const nextMessage = messages[count]!;
    const nextFrameSize = frameSize + nextMessage.length;
    if (nextFrameSize > maxFrameBytes) {
      break;
    }
    frameSize = nextFrameSize;
    count += 1;
  }
  return count;
}

export function encodeClientMessagesV3(
  writer: BinaryWriter,
  messages: readonly Uint8Array<ArrayBuffer>[],
  messageCount: number = messages.length
): Uint8Array<ArrayBuffer> {
  return concatenateMessagesV3(writer, messages, messageCount);
}

export function decodeClientMessagesV3(
  data: Uint8Array<ArrayBuffer>
): Uint8Array<ArrayBuffer>[] {
  return splitMessagesV3(new BinaryReader(data), data, reader =>
    ClientMessage.deserialize(reader)
  );
}

export function encodeServerMessagesV3(
  writer: BinaryWriter,
  messages: readonly Uint8Array<ArrayBuffer>[]
): Uint8Array<ArrayBuffer> {
  return concatenateMessagesV3(writer, messages);
}

export function forEachServerMessageV3(
  reader: BinaryReader,
  data: Uint8Array,
  visit: (message: ServerMessage) => void
): number {
  reader.reset(data);
  if (reader.remaining === 0) {
    throw new RangeError(EMPTY_V3_PAYLOAD_ERR);
  }

  let count = 0;
  while (reader.remaining > 0) {
    visit(ServerMessage.deserialize(reader));
    count += 1;
  }
  return count;
}
