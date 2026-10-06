using SpacetimeDB;
using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;
using Xunit;

// Rows from a module that added a column to the end of a table, read by a client built against the
// old schema (`PerfRow`, which only has `Id`).
public class RowListTests
{
    static byte[] EncodeRowWithAddedColumns(uint id, string? name)
    {
        using var stream = new MemoryStream();
        using var writer = new BinaryWriter(stream);
        new U32().Write(writer, id);
        if (name != null)
        {
            new SpacetimeDB.BSATN.String().Write(writer, name);
        }
        new U32().Write(writer, 0xDEAD);
        return stream.ToArray();
    }

    static uint[] ReadIds(BsatnRowList list)
    {
        var (reader, count) = CompressionHelpers.ParseRowList(list);
        var ids = new uint[count];
        for (var i = 0; i < count; i++)
        {
            CompressionHelpers.SeekRow(reader, list, i);
            ids[i] = new PerformanceTests.PerfRow.BSATN().Read(reader).Id;
        }
        return ids;
    }

    [Fact]
    public void ReadsVariableSizeRowsWithAddedColumns()
    {
        var rows = new[] { EncodeRowWithAddedColumns(1, "bob"), EncodeRowWithAddedColumns(2, "sally"), EncodeRowWithAddedColumns(3, "") };
        var offsets = new List<ulong>();
        var data = new List<byte>();
        foreach (var row in rows)
        {
            offsets.Add((ulong)data.Count);
            data.AddRange(row);
        }

        Assert.Equal(new uint[] { 1, 2, 3 }, ReadIds(new BsatnRowList(new RowSizeHint.RowOffsets(offsets), data)));
    }

    [Fact]
    public void ReadsFixedSizeRowsWithAddedColumns()
    {
        var rows = new[] { EncodeRowWithAddedColumns(1, null), EncodeRowWithAddedColumns(2, null) };
        var data = rows.SelectMany(row => row).ToList();

        Assert.Equal(new uint[] { 1, 2 }, ReadIds(new BsatnRowList(new RowSizeHint.FixedSize((ushort)rows[0].Length), data)));
    }
}
