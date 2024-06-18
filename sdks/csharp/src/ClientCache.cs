using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB.BSATN;
using Google.Protobuf;

namespace SpacetimeDB
{
    internal class ClientCache
    {
        public interface IDbValue
        {
            bool InsertEntry();
            bool DeleteEntry();
            IDatabaseTable Value { get; }
            ReadOnlyMemory<byte> Bytes { get; }
        }

        public interface ITableCache
        {
            Type ClientTableType { get; }
            IDbValue DecodeValue(ReadOnlyMemory<byte> bytes);
            IEnumerable<IDbValue> GetEntries();
            void Clear();
        }

        public class TableCache<T> : ITableCache
            where T : IDatabaseTable, IStructuralReadWrite, new()
        {
            public sealed record DbValue(ReadOnlyMemory<byte> Bytes, T Value) : IDbValue
            {
                IDatabaseTable IDbValue.Value => Value;

                public DbValue(ReadOnlyMemory<byte> bytes) : this(bytes, BSATNHelpers.FromBytes<T>(bytes))
                {
                }

                public bool InsertEntry()
                {
                    if (Entries.Add(this))
                    {
                        Value.InternalOnValueInserted();
                        return true;
                    }
                    return false;
                }

                public bool DeleteEntry()
                {
                    if (Entries.Remove(this))
                    {
                        Value.InternalOnValueDeleted();
                        return true;
                    }

                    Logger.LogWarning("Deleting value that we don't have (no cached value available)");
                    return false;
                }

                public bool Equals(DbValue other) => ByteArrayComparer.Instance.Equals(Bytes, other.Bytes);

                public override int GetHashCode() => ByteArrayComparer.Instance.GetHashCode(Bytes);
            }

            public Type ClientTableType => typeof(T);

            private static readonly HashSet<DbValue> Entries = new();

            // The function to use for decoding a type value.
            public IDbValue DecodeValue(ReadOnlyMemory<byte> bytes) => new DbValue(bytes);

            IEnumerable<IDbValue> ITableCache.GetEntries() => Entries;

            public static int Count => Entries.Count;

            public static IEnumerable<T> GetValues() => Entries.Select(e => e.Value);

            void ITableCache.Clear() => Entries.Clear();
        }

        private readonly Dictionary<string, ITableCache> tables = new();

        public void AddTable<T>()
            where T : IDatabaseTable, IStructuralReadWrite, new()
        {
            string name = typeof(T).Name;

            if (!tables.TryAdd(name, new TableCache<T>()))
            {
                Logger.LogError($"Table with name already exists: {name}");
            }
        }

        public ITableCache? GetTable(string name)
        {
            if (tables.TryGetValue(name, out var table))
            {
                return table;
            }

            Logger.LogError($"We don't know that this table is: {name}");
            return null;
        }

        public IEnumerable<ITableCache> GetTables() => tables.Values;
    }
}
