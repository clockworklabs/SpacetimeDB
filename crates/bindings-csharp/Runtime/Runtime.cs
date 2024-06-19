namespace SpacetimeDB;

using System;
using System.Collections;
using System.Collections.Frozen;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using SpacetimeDB.BSATN;
using SpacetimeDB.Module;
using static System.Text.Encoding;

public static partial class Runtime
{
    [Type]
    public enum IndexType : byte
    {
        BTree,
        Hash,
    }

    internal static byte[] Consume(this RawBindings.Buffer buffer)
    {
        var len = RawBindings._buffer_len(buffer);
        var result = new byte[len];
        RawBindings._buffer_consume(buffer, result, len);
        return result;
    }

    private class RowIter(RawBindings.RowIter handle) : IEnumerator<byte[]>, IDisposable
    {
        private byte[] buffer = new byte[0x20_000];
        public byte[] Current { get; private set; } = [];

        object IEnumerator.Current => Current;

        public bool MoveNext()
        {
            uint buffer_len;
            while (true)
            {
                buffer_len = (uint)buffer.Length;
                try
                {
                    RawBindings._iter_advance(handle, buffer, ref buffer_len);
                }
                catch (RawBindings.BufferTooSmallException)
                {
                    buffer = new byte[buffer_len];
                    continue;
                }
                break;
            }
            Current = new byte[buffer_len];
            Array.Copy(buffer, 0, Current, 0, buffer_len);
            return buffer_len != 0;
        }

        public void Dispose()
        {
            if (!handle.Equals(RawBindings.RowIter.INVALID))
            {
                RawBindings._iter_drop(handle);
                handle = RawBindings.RowIter.INVALID;
                // Avoid running ~RowIter if Dispose was executed successfully.
                GC.SuppressFinalize(this);
            }
        }

        // Free unmanaged resource just in case user hasn't disposed for some reason.
        ~RowIter()
        {
            // we already guard against double-free in Dispose.
            Dispose();
        }

        public void Reset()
        {
            throw new NotImplementedException();
        }
    }

    private abstract class RawTableIterBase : IEnumerable<byte[]>
    {
        protected abstract void IterStart(out RawBindings.RowIter handle);

        public IEnumerator<byte[]> GetEnumerator()
        {
            IterStart(out var handle);
            return new RowIter(handle);
        }

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        private static IEnumerable<T> ParseChunk<T>(byte[] chunk)
            where T : IStructuralReadWrite, new()
        {
            using var stream = new MemoryStream(chunk);
            using var reader = new BinaryReader(stream);
            while (stream.Position < stream.Length)
            {
                yield return IStructuralReadWrite.Read<T>(reader);
            }
        }

        public IEnumerable<T> Parse<T>()
            where T : IStructuralReadWrite, new()
        {
            return this.SelectMany(ParseChunk<T>);
        }
    }

    private class RawTableIter(RawBindings.TableId tableId) : RawTableIterBase
    {
        protected override void IterStart(out RawBindings.RowIter handle) =>
            RawBindings._iter_start(tableId, out handle);
    }

    private class RawTableIterFiltered(RawBindings.TableId tableId, byte[] filterBytes)
        : RawTableIterBase
    {
        protected override void IterStart(out RawBindings.RowIter handle) =>
            RawBindings._iter_start_filtered(
                tableId,
                filterBytes,
                (uint)filterBytes.Length,
                out handle
            );
    }

    private class RawTableIterByColEq(
        RawBindings.TableId tableId,
        RawBindings.ColId colId,
        byte[] value
    ) : RawTableIterBase
    {
        protected override void IterStart(out RawBindings.RowIter handle) =>
            RawBindings._iter_by_col_eq(tableId, colId, value, (uint)value.Length, out handle);
    }

    public interface IQueryField
    {
        public RawBindings.ColId ColId { get; }
        public string Name { get; }
        public ColumnAttrs ColumnAttrs { get; }
        AlgebraicType GetAlgebraicType(ITypeRegistrar registrar);
        void Write(BinaryWriter writer, object? value);
    }

    public interface IDatabaseTable<T> : IStructuralReadWrite
        where T : IDatabaseTable<T>, new()
    {
        protected static abstract AlgebraicType.Ref GetAlgebraicTypeRef(ITypeRegistrar registrar);
        protected static abstract bool IsPublic { get; }
        protected internal static abstract IQueryField[] QueryFields { get; }

        internal static TableDesc MakeTableDesc(ITypeRegistrar registrar) =>
            new(
                new(
                    typeof(T).Name,
                    T.QueryFields.Select(f => new ColumnDefWithAttrs(
                        new(f.Name, f.GetAlgebraicType(registrar)),
                        f.ColumnAttrs
                    ))
                        .ToArray(),
                    T.IsPublic
                ),
                T.GetAlgebraicTypeRef(registrar)
            );
    }

    public abstract class DatabaseTable<T>
        where T : DatabaseTable<T>, IDatabaseTable<T>, new()
    {
        // Note: this needs to be Lazy because we shouldn't accidentally invoke get_table_id during startup, when module isn't ready.
        private static readonly Lazy<RawBindings.TableId> tableIdLazy =
            new(() =>
            {
                var name_bytes = UTF8.GetBytes(typeof(T).Name);
                RawBindings._get_table_id(name_bytes, (uint)name_bytes.Length, out var out_);
                return out_;
            });

        private static RawBindings.TableId TableId => tableIdLazy.Value;

        public static IEnumerable<T> Iter() => new RawTableIter(TableId).Parse<T>();

        public static IEnumerable<T> Query(
            System.Linq.Expressions.Expression<Func<T, bool>> filter
        ) =>
            new RawTableIterFiltered(
                TableId,
                Filter.Filter.Compile(T.QueryFields, filter)
            ).Parse<T>();

        private static readonly bool HasAutoIncFields = T.QueryFields.Any(f =>
            f.ColumnAttrs.HasFlag(ColumnAttrs.AutoInc)
        );

        public void Insert()
        {
            var row = (T)this;
            var stream = new MemoryStream();
            var writer = new BinaryWriter(stream);
            row.WriteFields(writer);
            // Note: this gets an oversized buffer, we must send only the actual written length.
            var bytes = stream.GetBuffer();
            RawBindings._insert(TableId, bytes, (uint)stream.Position);
            // If we had autoinc fields, we need to parse the row back to update them.
            if (HasAutoIncFields)
            {
                // Reuse the same stream.
                stream.Seek(0, SeekOrigin.Begin);
                var reader = new BinaryReader(stream);
                row.ReadFields(reader);
            }
        }

        public record DatabaseColumn<Col, TColRW>(
            RawBindings.ColId ColId,
            string Name,
            ColumnAttrs ColumnAttrs
        ) : IQueryField
            where TColRW : IReadWrite<Col>, new()
        {
            private static readonly TColRW ColRW = new();

            public readonly ref struct WithValueRef(DatabaseColumn<Col, TColRW> column, Col value)
            {
                private readonly RawBindings.ColId ColId => column.ColId;
                private readonly byte[] Value = IStructuralReadWrite.ToBytes(ColRW, value);

                public IEnumerable<T> FilterBy() =>
                    new RawTableIterByColEq(TableId, ColId, Value).Parse<T>();

                public T? FindBy() => FilterBy().Cast<T?>().SingleOrDefault();

                public bool DeleteBy()
                {
                    RawBindings._delete_by_col_eq(
                        TableId,
                        ColId,
                        Value,
                        (uint)Value.Length,
                        out var out_
                    );
                    return out_ > 0;
                }

                public bool UpdateBy(T row)
                {
                    // Just like in Rust bindings, updating is just deleting and inserting for now.
                    if (DeleteBy())
                    {
                        row.Insert();
                        return true;
                    }
                    else
                    {
                        return false;
                    }
                }
            }

            public WithValueRef WithValue(Col value) => new(this, value);

            public void Write(BinaryWriter writer, object? value) =>
                ColRW.Write(writer, (Col)value!);

            public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
                ColRW.GetAlgebraicType(registrar);
        }
    }

    public enum LogLevel : byte
    {
        Error,
        Warn,
        Info,
        Debug,
        Trace,
        Panic
    }

    public static void Log(
        string text,
        LogLevel level = LogLevel.Info,
        [CallerMemberName] string target = "",
        [CallerFilePath] string filename = "",
        [CallerLineNumber] uint lineNumber = 0
    )
    {
        var target_bytes = UTF8.GetBytes(target);
        var filename_bytes = UTF8.GetBytes(filename);
        var text_bytes = UTF8.GetBytes(text);

        RawBindings._console_log(
            (byte)level,
            target_bytes,
            (uint)target_bytes.Length,
            filename_bytes,
            (uint)filename_bytes.Length,
            lineNumber,
            text_bytes,
            (uint)text_bytes.Length
        );
    }

    public readonly struct Identity(byte[] bytes) : IEquatable<Identity>
    {
        private readonly byte[] bytes = bytes;

        public bool Equals(Identity other) => bytes.SequenceEqual(other.bytes);

        public override bool Equals(object? obj) => obj is Identity other && Equals(other);

        public static bool operator ==(Identity left, Identity right) => left.Equals(right);

        public static bool operator !=(Identity left, Identity right) => !left.Equals(right);

        public override int GetHashCode() =>
            StructuralComparisons.StructuralEqualityComparer.GetHashCode(bytes);

        public override string ToString() => BitConverter.ToString(bytes);

        // We need to implement this manually because `spacetime generate` only
        // recognises inline product type with special field name and
        // not types registered on ModuleDef (which is what [SpacetimeDB.Type] does).
        public readonly struct BSATN : IReadWrite<Identity>
        {
            public Identity Read(BinaryReader reader) => new(ByteArray.Instance.Read(reader));

            public void Write(BinaryWriter writer, Identity value) =>
                ByteArray.Instance.Write(writer, value.bytes);

            public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
                new AlgebraicType.Product(
                    // Special name recognised by STDB generator.
                    [new("__identity_bytes", ByteArray.Instance.GetAlgebraicType(registrar))]
                );
        }
    }

    public readonly struct Address(byte[] bytes) : IEquatable<Address>
    {
        private readonly byte[] bytes = bytes;

        public static Address? From(byte[] bytes) => bytes.All(b => b == 0) ? null : new(bytes);

        public bool Equals(Address other) => bytes.SequenceEqual(other.bytes);

        public override bool Equals(object? obj) => obj is Address other && Equals(other);

        public static bool operator ==(Address left, Address right) => left.Equals(right);

        public static bool operator !=(Address left, Address right) => !left.Equals(right);

        public override int GetHashCode() =>
            StructuralComparisons.StructuralEqualityComparer.GetHashCode(bytes);

        public override string ToString() => BitConverter.ToString(bytes);

        // We need to implement this manually because `spacetime generate` only
        // recognises inline product type with special field name and
        // not types registered on ModuleDef (which is what [SpacetimeDB.Type] does).
        public readonly struct BSATN : IReadWrite<Address>
        {
            public Address Read(BinaryReader reader) => new(ByteArray.Instance.Read(reader));

            public void Write(BinaryWriter writer, Address value) =>
                ByteArray.Instance.Write(writer, value.bytes);

            public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
                new AlgebraicType.Product(
                    // Special name recognised by STDB generator.
                    [new("__address_bytes", ByteArray.Instance.GetAlgebraicType(registrar))]
                );
        }
    }

    public class ReducerContext(byte[] senderIdentity, byte[] senderAddress, ulong timestamp_us)
    {
        public readonly Identity Sender = new(senderIdentity);
        public readonly DateTimeOffset Time = DateTimeOffset.UnixEpoch.AddTicks(
            10 * (long)timestamp_us
        );
        public readonly Address? Address = Runtime.Address.From(senderAddress);
    }

    public class ScheduleToken
    {
        private readonly RawBindings.ScheduleToken handle;

        public ScheduleToken(string name, byte[] args, DateTimeOffset time)
        {
            var name_bytes = UTF8.GetBytes(name);

            RawBindings._schedule_reducer(
                name_bytes,
                (uint)name_bytes.Length,
                args,
                (uint)args.Length,
                (ulong)((time - DateTimeOffset.UnixEpoch).Ticks / 10),
                out handle
            );
        }

        public void Cancel() => RawBindings._cancel_reducer(handle);
    }
}
