using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB.Internal;

namespace SpacetimeDB
{
    public class ClientCache
    {
        public interface ITableCache : IEnumerable<KeyValuePair<byte[], IDatabaseRow>>
        {
            Type ClientTableType { get; }
            bool InsertEntry(byte[] rowBytes, IDatabaseRow value);
            bool DeleteEntry(byte[] rowBytes);
            IDatabaseRow DecodeValue(byte[] bytes);
            IRemoteTableHandle Handle { get; }
        }

        public class TableCache<Row> : ITableCache
            where Row : IDatabaseRow, new()
        {
            public TableCache(IRemoteTableHandle handle) => Handle = handle;

            public IRemoteTableHandle Handle { get; init; }

            public Type ClientTableType => typeof(Row);

            public readonly Dictionary<byte[], Row> Entries = new(ByteArrayComparer.Instance);

            /// <summary>
            /// Inserts the value into the table. There can be no existing value with the provided BSATN bytes.
            /// </summary>
            /// <param name="rowBytes">The BSATN encoded bytes of the row to retrieve.</param>
            /// <param name="value">The parsed row encoded by the <paramref>rowBytes</paramref>.</param>
            /// <returns>True if the row was inserted, false if the row wasn't inserted because it was a duplicate.</returns>
            public bool InsertEntry(byte[] rowBytes, IDatabaseRow value) => Entries.TryAdd(rowBytes, (Row)value);

            /// <summary>
            /// Deletes a value from the table.
            /// </summary>
            /// <param name="rowBytes">The BSATN encoded bytes of the row to remove.</param>
            /// <returns>True if and only if the value was previously resident and has been deleted.</returns>
            public bool DeleteEntry(byte[] rowBytes)
            {
                if (Entries.Remove(rowBytes))
                {
                    return true;
                }

                Log.Warn("Deleting value that we don't have (no cached value available)");
                return false;
            }

            // The function to use for decoding a type value.
            public IDatabaseRow DecodeValue(byte[] bytes) => BSATNHelpers.Decode<Row>(bytes);

            public IEnumerator<KeyValuePair<byte[], IDatabaseRow>> GetEnumerator() => Entries.Select(kv => new KeyValuePair<byte[], IDatabaseRow>(kv.Key, kv.Value)).GetEnumerator();

            IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
        }

        private readonly Dictionary<string, ITableCache> tables = new();

        public void AddTable<Row>(string name, IRemoteTableHandle handle)
            where Row : IDatabaseRow, new()
        {
            var cache = new TableCache<Row>(handle);
            handle.SetCache(cache);
            if (!tables.TryAdd(name, cache))
            {
                Log.Error($"Table with name already exists: {name}");
            }
        }

        public ITableCache? GetTable(string name)
        {
            if (tables.TryGetValue(name, out var table))
            {
                return table;
            }

            Log.Error($"We don't know that this table is: {name}");
            return null;
        }

        public IEnumerable<ITableCache> GetTables() => tables.Values;
    }
}
