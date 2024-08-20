﻿namespace SpacetimeDB;

using System.Collections;
using System.Runtime.CompilerServices;

using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;

public abstract class BaseReducerContext<DbView> : DbContext<DbView> where DbView : struct
{
    public Identity Sender => Runtime.SenderIdentity!;
    public Address Address => Runtime.SenderAddress!;
    public DateTimeOffset Timestamp => Runtime.Timestamp!;
}

internal static class Tmp
{
    internal static MemoryStream stream = new(0x20_000);
    internal static BinaryReader reader = new(stream);

    static Tmp()
    {
        stream.SetLength(stream.Capacity);
    }
}

public struct LocalTableIter<T>(FFI.TableId id) : IEnumerable<T> where T : struct, IStructuralReadWrite
{
    public readonly Enumerator GetEnumerator()
    {
        FFI._iter_start(id, out var handle);
        return new(handle);
    }

    readonly IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

    readonly IEnumerator<T> IEnumerable<T>.GetEnumerator() => GetEnumerator();

    public struct Enumerator(FFI.RowIter handle) : IEnumerator<T>
    {
        public T Current { get; private set; }

        readonly object IEnumerator.Current => Current;

        public void Dispose()
        {
            if (handle != FFI.RowIter.INVALID)
            {
                FFI._iter_drop(handle);
                handle = FFI.RowIter.INVALID;
                GC.SuppressFinalize(this);
            }
        }

        long _offset;

        public bool MoveNext()
        {
            if (Tmp.stream.Position >= _offset)
            {
                uint len = (uint)Tmp.stream.Capacity; retry:
                var buffer = Tmp.stream.GetBuffer();
                try
                {
                    FFI._iter_advance(handle, buffer, ref len);
                }
                catch (BufferTooSmallException)
                {
                    Runtime.Log($"Resized the row enumerator buffer to {len} bytes", Runtime.LogLevel.Debug);
                    Tmp.stream.Capacity = (int)len;
                    Tmp.stream.SetLength((int)len);
                    goto retry;
                }

                if (len == 0)
                {
                    return false;
                }

                Tmp.stream.Position = 0;
            }

            var result = new T();
            result.ReadFields(Tmp.reader);
            Current = result;
            _offset = Tmp.stream.Position;
            return true;
        }

        public void Reset() => throw new NotSupportedException();
    }
}