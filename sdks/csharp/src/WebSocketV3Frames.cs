using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;

using System;
using System.Collections.Generic;
using System.IO;

namespace SpacetimeDB
{
    internal static class WebSocketV3Frames
    {
        internal const int MaxFrameBytes = 256 * 1024;

        private const int EnumTagBytes = 1;
        private const int CollectionLengthBytes = 4;
        private const int ByteArrayLengthBytes = 4;

        private static readonly ClientFrame.BSATN clientFrameBsatn = new();
        private static readonly ServerFrame.BSATN serverFrameBsatn = new();

        // v3 is only a transport envelope around already-encoded v2 messages,
        // so batching works in terms of raw byte payloads rather than logical messages.
        internal static byte[] EncodeClientMessages(IReadOnlyList<byte[]> messages)
        {
            if (messages.Count == 0)
            {
                throw new InvalidOperationException("Cannot encode an empty v3 client frame.");
            }

            ClientFrame frame = messages.Count == 1
                ? new ClientFrame.Single(messages[0])
                : new ClientFrame.Batch(ToArray(messages));

            return IStructuralReadWrite.ToBytes(clientFrameBsatn, frame);
        }

        internal static byte[][] DecodeServerMessages(byte[] encodedFrame)
        {
            using var stream = new MemoryStream(encodedFrame);
            using var reader = new BinaryReader(stream);
            var frame = serverFrameBsatn.Read(reader);
            return frame switch
            {
                ServerFrame.Single(var message) => new[] { message },
                ServerFrame.Batch(var messages) => messages,
                _ => throw new InvalidOperationException("Unknown v3 server frame variant."),
            };
        }

        // Count the maximal prefix of already-encoded client messages that fits in
        // one v3 frame using BSATN framing sizes directly instead of trial serialization.
        internal static int CountClientMessagesThatFitInFrame(
            IEnumerable<byte[]> messages,
            int maxFrameBytes = MaxFrameBytes
        )
        {
            var messageCount = 0;
            var payloadBytes = 0;

            foreach (var message in messages)
            {
                if (messageCount == 0)
                {
                    if (EncodedSingleFrameSize(message.Length) > maxFrameBytes)
                    {
                        return 1;
                    }
                }
                else
                {
                    var batchSize = EncodedBatchFrameSize(messageCount + 1, payloadBytes + message.Length);
                    if (batchSize > maxFrameBytes)
                    {
                        break;
                    }
                }

                messageCount++;
                payloadBytes += message.Length;
            }

            return messageCount;
        }

        private static int EncodedSingleFrameSize(int messageBytes) =>
            EnumTagBytes + ByteArrayLengthBytes + messageBytes;

        private static int EncodedBatchFrameSize(int messageCount, int payloadBytes) =>
            EnumTagBytes + CollectionLengthBytes + (messageCount * ByteArrayLengthBytes) + payloadBytes;

        private static byte[][] ToArray(IReadOnlyList<byte[]> messages)
        {
            var array = new byte[messages.Count][];
            for (var i = 0; i < messages.Count; i++)
            {
                array[i] = messages[i];
            }
            return array;
        }
    }
}
