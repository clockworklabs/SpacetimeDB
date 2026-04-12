import BinaryReader from '../lib/binary_reader.ts';
import BinaryWriter from '../lib/binary_writer.ts';
import {
  ClientFrame,
  ServerFrame,
  type ClientFrame as ClientFrameValue,
  type ServerFrame as ServerFrameValue,
} from './client_api/v3';

// v3 is only a transport envelope. The inner payloads are already-encoded v2
// websocket messages, so these helpers intentionally operate on raw bytes.
type V3FrameValue = ClientFrameValue | ServerFrameValue;

function flattenFrame(frame: V3FrameValue): Uint8Array[] {
  return frame.tag === 'Single' ? [frame.value] : frame.value;
}

function ensureMessages(messages: readonly Uint8Array[]): void {
  if (messages.length === 0) {
    throw new RangeError(
      'v3 websocket frames must contain at least one message'
    );
  }
}

const BSATN_SUM_TAG_BYTES = 1;
const BSATN_LENGTH_PREFIX_BYTES = 4;

function encodedSingleFrameSize(message: Uint8Array): number {
  return BSATN_SUM_TAG_BYTES + BSATN_LENGTH_PREFIX_BYTES + message.length;
}

function encodedBatchFrameSizeForFirstMessage(message: Uint8Array): number {
  return (
    BSATN_SUM_TAG_BYTES +
    BSATN_LENGTH_PREFIX_BYTES +
    BSATN_LENGTH_PREFIX_BYTES +
    message.length
  );
}

function encodedBatchElementSize(message: Uint8Array): number {
  return BSATN_LENGTH_PREFIX_BYTES + message.length;
}

export function countClientMessagesForV3Frame(
  messages: readonly Uint8Array[],
  maxFrameBytes: number
): number {
  ensureMessages(messages);

  const firstMessage = messages[0]!;
  if (encodedSingleFrameSize(firstMessage) > maxFrameBytes) {
    return 1;
  }

  let count = 1;
  let batchSize = encodedBatchFrameSizeForFirstMessage(firstMessage);
  while (count < messages.length) {
    const nextMessage = messages[count]!;
    const nextBatchSize = batchSize + encodedBatchElementSize(nextMessage);
    if (nextBatchSize > maxFrameBytes) {
      break;
    }
    batchSize = nextBatchSize;
    count += 1;
  }
  return count;
}

export function encodeClientMessagesV3(
  writer: BinaryWriter,
  messages: readonly Uint8Array[]
): Uint8Array {
  ensureMessages(messages);
  writer.clear();
  if (messages.length === 1) {
    ClientFrame.serialize(writer, ClientFrame.Single(messages[0]!));
  } else {
    ClientFrame.serialize(writer, ClientFrame.Batch(Array.from(messages)));
  }
  return writer.getBuffer();
}

export function decodeClientMessagesV3(data: Uint8Array): Uint8Array[] {
  return flattenFrame(ClientFrame.deserialize(new BinaryReader(data)));
}

export function encodeServerMessagesV3(
  writer: BinaryWriter,
  messages: readonly Uint8Array[]
): Uint8Array {
  ensureMessages(messages);
  writer.clear();
  if (messages.length === 1) {
    ServerFrame.serialize(writer, ServerFrame.Single(messages[0]!));
  } else {
    ServerFrame.serialize(writer, ServerFrame.Batch(Array.from(messages)));
  }
  return writer.getBuffer();
}

export function decodeServerMessagesV3(
  reader: BinaryReader,
  data: Uint8Array
): Uint8Array[] {
  reader.reset(data);
  return flattenFrame(ServerFrame.deserialize(reader));
}
