using System.Diagnostics;
using System.IO.Compression;
using SpacetimeDB;
using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;
using Xunit;
using Xunit.Abstractions;

public sealed class PerformanceTests
{
    private readonly ITestOutputHelper output;

    public PerformanceTests(ITestOutputHelper output)
    {
        this.output = output;
    }

    [Theory]
    [InlineData(1)]
    [InlineData(100)]
    [InlineData(10_000)]
    public void ParseRowListBaseline(int rowCount)
    {
        var rowList = MakeRowList(rowCount);

        Measure($"ParseRowList rows={rowCount}", Iterations(rowCount), () =>
        {
            var (reader, count) = CompressionHelpers.ParseRowList(rowList);
            for (var i = 0; i < count; i++)
            {
                new PerfRow.BSATN().Read(reader);
            }
        });
    }

    [Theory]
    [InlineData(1)]
    [InlineData(100)]
    [InlineData(10_000)]
    public void MultiDictionaryApplyBaseline(int rowCount)
    {
        Measure($"MultiDictionary.Apply rows={rowCount}", Iterations(rowCount), () =>
        {
            var dict = new MultiDictionary<uint, PerfRow>(EqualityComparer<uint>.Default, EqualityComparer<PerfRow>.Default);
            var delta = new MultiDictionaryDelta<uint, PerfRow>(EqualityComparer<uint>.Default, EqualityComparer<PerfRow>.Default);
            for (uint i = 0; i < rowCount; i++)
            {
                delta.Add(i, new PerfRow { Id = i });
            }

            var inserted = new List<KeyValuePair<uint, PerfRow>>();
            var updated = new List<(uint key, PerfRow oldValue, PerfRow newValue)>();
            var removed = new List<KeyValuePair<uint, PerfRow>>();
            dict.Apply(delta, inserted, updated, removed);
        });
    }

    [Theory]
    [InlineData(1)]
    [InlineData(100)]
    [InlineData(10_000)]
    public void PayloadSerializationAndProcedureDecodeBaseline(int rowCount)
    {
        var rows = Enumerable.Range(0, rowCount).Select(i => new PerfRow { Id = (uint)i }).ToList();

        Measure($"Reducer/procedure payload rows={rowCount}", Iterations(rowCount), () =>
        {
            var encoded = IStructuralReadWrite.ToBytes(new PerfRows.BSATN(), new PerfRows { Rows = rows }).ToList();
            _ = BSATNHelpers.Decode<PerfRows>(encoded);
        });
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void WebsocketMessageDecodeBaseline(bool brotli)
    {
        var message = new ServerMessage.InitialConnection(new InitialConnection
        {
            Identity = Identity.From(Convert.FromBase64String("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=")),
            Token = "token",
            ConnectionId = ConnectionId.From(Convert.FromBase64String("Vd4dFzcEzhLHJ6uNL8VXFg=="))
                ?? throw new InvalidDataException("connection id"),
        });
        var bytes = EncodeServerMessage(message, brotli);

        Measure($"DecompressDecodeMessage brotli={brotli}", 10_000, () =>
        {
            _ = CompressionHelpers.DecompressDecodeMessage(bytes);
        });
    }

    private void Measure(string name, int iterations, Action action)
    {
        action();
        GC.Collect();
        GC.WaitForPendingFinalizers();
        GC.Collect();

        var gen0 = GC.CollectionCount(0);
        var gen1 = GC.CollectionCount(1);
        var allocatedBefore = GC.GetAllocatedBytesForCurrentThread();
        var stopwatch = Stopwatch.StartNew();

        for (var i = 0; i < iterations; i++)
        {
            action();
        }

        stopwatch.Stop();
        var allocatedBytes = GC.GetAllocatedBytesForCurrentThread() - allocatedBefore;
        var meanMicroseconds = stopwatch.Elapsed.TotalMilliseconds * 1000 / iterations;
        var allocatedBytesPerOp = allocatedBytes / iterations;

        output.WriteLine(
            $"{name}: mean={meanMicroseconds:F2}us allocated={allocatedBytesPerOp}B/op gen0={GC.CollectionCount(0) - gen0} gen1={GC.CollectionCount(1) - gen1} iterations={iterations}"
        );
        Assert.True(iterations > 0);
    }

    private static int Iterations(int rowCount) =>
        rowCount switch
        {
            <= 1 => 10_000,
            <= 100 => 1_000,
            _ => 10,
        };

    private static BsatnRowList MakeRowList(int rowCount)
    {
        var bytes = new List<byte>(rowCount * sizeof(uint));
        using var stream = new MemoryStream();
        using var writer = new BinaryWriter(stream);
        var rowRW = new PerfRow.BSATN();
        for (uint i = 0; i < rowCount; i++)
        {
            rowRW.Write(writer, new PerfRow { Id = i });
        }
        bytes.AddRange(stream.ToArray());
        return new BsatnRowList(new RowSizeHint.FixedSize(sizeof(uint)), bytes);
    }

    private static byte[] EncodeServerMessage(ServerMessage message, bool brotli)
    {
        using var output = new MemoryStream();
        output.WriteByte(brotli ? (byte)1 : (byte)0);
        if (brotli)
        {
            using (var compressed = new BrotliStream(output, CompressionMode.Compress, leaveOpen: true))
            using (var writer = new BinaryWriter(compressed))
            {
                new ServerMessage.BSATN().Write(writer, message);
            }
        }
        else
        {
            using var writer = new BinaryWriter(output);
            new ServerMessage.BSATN().Write(writer, message);
        }
        return output.ToArray();
    }

    public sealed class PerfRow : IStructuralReadWrite, IEquatable<PerfRow>
    {
        public uint Id;

        public void ReadFields(BinaryReader reader)
        {
            Id = new U32().Read(reader);
        }

        public void WriteFields(BinaryWriter writer)
        {
            new U32().Write(writer, Id);
        }

        public object GetSerializer() => new BSATN();

        public bool Equals(PerfRow? other) => other != null && Id == other.Id;

        public override bool Equals(object? obj) => obj is PerfRow other && Equals(other);

        public override int GetHashCode() => Id.GetHashCode();

        public readonly struct BSATN : IReadWrite<PerfRow>
        {
            public PerfRow Read(BinaryReader reader) => new() { Id = new U32().Read(reader) };

            public void Write(BinaryWriter writer, PerfRow value) => new U32().Write(writer, value.Id);

            public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) => throw new NotImplementedException();
        }
    }

    public sealed class PerfRows : IStructuralReadWrite
    {
        public List<PerfRow> Rows = new();

        public void ReadFields(BinaryReader reader)
        {
            Rows = new SpacetimeDB.BSATN.List<PerfRow, PerfRow.BSATN>().Read(reader);
        }

        public void WriteFields(BinaryWriter writer)
        {
            new SpacetimeDB.BSATN.List<PerfRow, PerfRow.BSATN>().Write(writer, Rows);
        }

        public object GetSerializer() => new BSATN();

        public readonly struct BSATN : IReadWrite<PerfRows>
        {
            public PerfRows Read(BinaryReader reader) =>
                new() { Rows = new SpacetimeDB.BSATN.List<PerfRow, PerfRow.BSATN>().Read(reader) };

            public void Write(BinaryWriter writer, PerfRows value) =>
                new SpacetimeDB.BSATN.List<PerfRow, PerfRow.BSATN>().Write(writer, value.Rows);

            public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) => throw new NotImplementedException();
        }
    }
}
