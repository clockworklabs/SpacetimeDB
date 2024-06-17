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
            byte[] Bytes { get; }
        }

        public interface ITableCache : IEnumerable<IDbValue>
        {
            Type ClientTableType { get; }
            IDbValue DecodeValue(byte[] bytes);
            void Clear();
        }

        public class TableCache<T> : ITableCache
            where T : IDatabaseTable, IStructuralReadWrite, new()
        {
            public record DbValue(byte[] Bytes, T Value) : IDbValue
            {
                IDatabaseTable IDbValue.Value => Value;

                public DbValue(byte[] bytes) : this(bytes, BSATNHelpers.FromBytes<T>(bytes))
                {
                }

                public bool InsertEntry()
                {
                    if (Entries.TryAdd(Bytes, Value))
                    {
                        Value.InternalOnValueInserted();
                        return true;
                    }
                    return false;
                }

                public bool DeleteEntry()
                {
                    if (Entries.Remove(Bytes))
                    {
                        Value.InternalOnValueDeleted();
                        return true;
                    }

                    Logger.LogWarning("Deleting value that we don't have (no cached value available)");
                    return false;
                }
            }

            public Type ClientTableType => typeof(T);

            public static readonly Dictionary<byte[], T> Entries = new(ByteArrayComparer.Instance);

            // The function to use for decoding a type value.
            public IDbValue DecodeValue(byte[] bytes) => new DbValue(bytes);

            public IEnumerator<IDbValue> GetEnumerator() => Entries.Select(pair => new DbValue(pair.Key, pair.Value)).GetEnumerator();

            IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

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
