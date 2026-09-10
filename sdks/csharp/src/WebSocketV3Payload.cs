using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;

using System;
using System.Collections.Generic;
using System.IO;

namespace SpacetimeDB
{
    internal static class WebSocketV3Payload
    {
        internal const int MaxPayloadBytes = 256 * 1024;

        private static readonly ServerMessage.BSATN serverMessageBsatn = new();

        internal static byte[] EncodeClientMessages(IReadOnlyList<byte[]> messages)
        {
            if (messages.Count == 0)
            {
                throw new InvalidOperationException("Cannot encode an empty v3 client payload.");
            }

            var payloadBytes = 0;
            for (var i = 0; i < messages.Count; i++)
            {
                payloadBytes += messages[i].Length;
            }

            var payload = new byte[payloadBytes];
            var offset = 0;
            for (var i = 0; i < messages.Count; i++)
            {
                Buffer.BlockCopy(messages[i], 0, payload, offset, messages[i].Length);
                offset += messages[i].Length;
            }
            return payload;
        }

        internal static byte[][] DecodeServerMessages(byte[] payload)
        {
            if (payload.Length == 0)
            {
                throw new InvalidOperationException("Cannot decode an empty v3 server payload.");
            }

            using var stream = new MemoryStream(payload);
            using var reader = new BinaryReader(stream);
            var messages = new List<byte[]>();

            while (stream.Position < stream.Length)
            {
                var start = stream.Position;
                serverMessageBsatn.Read(reader);
                var end = stream.Position;

                var message = new byte[end - start];
                Buffer.BlockCopy(payload, (int)start, message, 0, message.Length);
                messages.Add(message);
            }

            return messages.ToArray();
        }

        internal static int CountClientMessagesThatFitInPayload(
            IEnumerable<byte[]> messages,
            int maxPayloadBytes = MaxPayloadBytes
        )
        {
            var messageCount = 0;
            var payloadBytes = 0;

            foreach (var message in messages)
            {
                if (messageCount == 0)
                {
                    if (message.Length > maxPayloadBytes)
                    {
                        return 1;
                    }
                }
                else if (payloadBytes + message.Length > maxPayloadBytes)
                {
                    break;
                }

                messageCount++;
                payloadBytes += message.Length;
            }

            return messageCount;
        }
    }
}
