using BenchmarkDotNet.Attributes;
using BenchmarkDotNet.Jobs;
using BenchmarkDotNet.Running;
using SpacetimeDB;
using SpacetimeDB.BSATN;

BenchmarkSwitcher.FromAssembly(typeof(Program).Assembly).Run(args);

[MemoryDiagnoser]
[SimpleJob(RuntimeMoniker.Net80, launchCount: 1, warmupCount: 5, iterationCount: 10)]
public class ReducerProcedurePayloadBenchmarks
{
    private readonly PerfRows.BSATN rowsRW = new();
    private List<PerfRow> rows = [];
    private byte[] encodedBytes = [];
    private List<byte> encodedList = [];
    private IReadOnlyList<byte> encodedReadOnlyList = [];

    [Params(1, 100, 10_000)]
    public int RowCount { get; set; }

    [GlobalSetup]
    public void GlobalSetup()
    {
        rows = Enumerable.Range(0, RowCount).Select(i => new PerfRow { Id = (uint)i }).ToList();
        encodedBytes = IStructuralReadWrite.ToBytes(rowsRW, new PerfRows { Rows = rows });
        encodedList = encodedBytes.ToList();
        encodedReadOnlyList = encodedList;
    }

    [Benchmark(Baseline = true)]
    public PerfRows XUnitReducerProcedureShape()
    {
        var encoded = IStructuralReadWrite.ToBytes(rowsRW, new PerfRows { Rows = rows }).ToList();
        return BSATNHelpers.Decode<PerfRows>(encoded);
    }

    [Benchmark]
    public PerfRows SerializeAndDecodeFromByteArray()
    {
        var encoded = IStructuralReadWrite.ToBytes(rowsRW, new PerfRows { Rows = rows });
        return BSATNHelpers.Decode<PerfRows>(encoded);
    }

    [Benchmark]
    public List<byte> SerializeToListOnly() =>
        IStructuralReadWrite.ToBytes(rowsRW, new PerfRows { Rows = rows }).ToList();

    [Benchmark]
    public PerfRows DecodeFromListOnly() =>
        BSATNHelpers.Decode<PerfRows>(encodedList);

    [Benchmark]
    public PerfRows DecodeFromReadOnlyListOnly() =>
        BSATNHelpers.Decode<PerfRows>(encodedReadOnlyList);

    [Benchmark]
    public PerfRows DecodeFromByteArrayOnly() =>
        BSATNHelpers.Decode<PerfRows>(encodedBytes);

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
        public List<PerfRow> Rows = [];

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
