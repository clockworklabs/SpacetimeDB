using System;
using Google.Protobuf;
using SpacetimeDB.BSATN;
using System.IO;

namespace SpacetimeDB
{
    public static class BSATNHelpers
    {
        public static T FromStream<T>(Stream stream)
            where T : IStructuralReadWrite, new()
        {
            using var reader = new BinaryReader(stream);
            return IStructuralReadWrite.Read<T>(reader);
        }

        // There is CommunityToolkit.HighPerformance that provides this,
        // but it's not compatible with Unity as it relies on .NET intrinsics,
        // so we need to implement our own.
        class ProtoStream : Stream
        {
            private readonly ReadOnlyMemory<byte> memory;

            public ProtoStream(ByteString input)
            {
                memory = input.Memory;
            }

            public override long Position { get; set; }
            public override long Length => memory.Length;

            public override bool CanRead => true;
            public override int Read(byte[] buffer, int offset, int count)
            {
                memory.Slice((int)Position, count).CopyTo(buffer.AsMemory(offset));
                Position += count;
                return count;
            }

            // Easy to implement, but not needed for our use cases.
            public override bool CanSeek => false;
            public override long Seek(long offset, SeekOrigin origin) => throw new NotImplementedException();


            // Our stream is read-only.
            public override bool CanWrite => false;
            public override void Write(byte[] buffer, int offset, int count) => throw new NotSupportedException();
            public override void Flush() { }
            public override void SetLength(long value) => throw new NotSupportedException();
        }

        public static T FromBytes<T>(byte[] bytes)
            where T : IStructuralReadWrite, new()
        {
            using var stream = new MemoryStream(bytes);
            return FromStream<T>(stream);
        }

        public static T FromProtoBytes<T>(ByteString bytes)
            where T : IStructuralReadWrite, new()
        {
            using var stream = new ProtoStream(bytes);
            return FromStream<T>(stream);
        }

        public static ByteString ToProtoBytes<T>(this T value)
            where T : IStructuralReadWrite, new()
        {
            using var stream = new MemoryStream();
            using var writer = new BinaryWriter(stream);
            value.WriteFields(writer);
            // This is safe because we know we own args so nobody else will modify it.
            return UnsafeByteOperations.UnsafeWrap(stream.ToArray());
        }
    }
}
