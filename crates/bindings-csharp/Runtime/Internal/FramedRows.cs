#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
namespace SpacetimeDB.Internal;

using System.Buffers.Binary;
using SpacetimeDB.BSATN;

/// <summary>
/// Decodes native BSATN <c>array&lt;array&lt;u8&gt;&gt;</c> row snapshots. Each row is read from a
/// slice of its declared length, and malformed input throws <see cref="InvalidDataException"/>.
/// </summary>
internal static class FramedRows
{
    internal static uint ReadCount(byte[] snapshot, ref int offset)
    {
        if (snapshot.Length - offset < sizeof(uint))
            throw new InvalidDataException("Truncated native row snapshot.");
        var value = BinaryPrimitives.ReadUInt32LittleEndian(snapshot.AsSpan(offset));
        offset += sizeof(uint);
        return value;
    }

    internal static T ReadRow<T>(byte[] snapshot, ref int offset)
        where T : IStructuralReadWrite, new()
    {
        var length = ReadCount(snapshot, ref offset);
        if (length > (uint)(snapshot.Length - offset))
            throw new InvalidDataException("Truncated native row snapshot.");
        // Slice the existing buffer so a row can't read into the next one.
        using var reader = new BinaryReader(
            new MemoryStream(snapshot, offset, (int)length, writable: false)
        );
        T row;
        try
        {
            row = IStructuralReadWrite.Read<T>(reader);
        }
        catch (EndOfStreamException e)
        {
            throw new InvalidDataException("Native row does not match its BSATN schema.", e);
        }
        if (reader.BaseStream.Position != length)
            throw new InvalidDataException("Native row does not match its BSATN schema.");
        offset += (int)length;
        return row;
    }

    internal static void CheckEnd(byte[] snapshot, int offset)
    {
        if (offset != snapshot.Length)
            throw new InvalidDataException("Trailing bytes in native row snapshot.");
    }
}
#endif
