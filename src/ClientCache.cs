using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Reflection;
using SpacetimeDB.SATS;
using System.Linq;

namespace SpacetimeDB
{
    public class ClientCache
    {
        public class TableCache
        {
            private readonly Type clientTableType;
            private readonly AlgebraicType rowSchema;

            // The function to use for decoding a type value
            private Func<AlgebraicValue, object> decoderFunc;

            // Maps from primary key to type value
            public readonly Dictionary<byte[], object> entries = new(ByteArrayComparer.Instance);

            public Type ClientTableType
            {
                get => clientTableType;
            }

            public Action<object> InternalValueInsertedCallback;
            public Action<object> InternalValueDeletedCallback;
            public Action<object, ClientApi.Event> InsertCallback;
            public Action<object, ClientApi.Event> BeforeDeleteCallback;
            public Action<object, ClientApi.Event> DeleteCallback;
            public Action<object, object, ClientApi.Event> UpdateCallback;
            public Func<object, object> GetPrimaryKeyValueFunc;

            public AlgebraicType RowSchema
            {
                get => rowSchema;
            }

            public TableCache(Type clientTableType, AlgebraicType rowSchema, Func<AlgebraicValue, object> decoderFunc)
            {
                this.clientTableType = clientTableType;

                this.rowSchema = rowSchema;
                this.decoderFunc = decoderFunc;
                InternalValueInsertedCallback = (Action<object>)clientTableType.GetMethod("InternalOnValueInserted", BindingFlags.NonPublic | BindingFlags.Static)?.CreateDelegate(typeof(Action<object>));
                InternalValueDeletedCallback = (Action<object>)clientTableType.GetMethod("InternalOnValueDeleted", BindingFlags.NonPublic | BindingFlags.Static)?.CreateDelegate(typeof(Action<object>));
                InsertCallback = (Action<object, ClientApi.Event>)clientTableType.GetMethod("OnInsertEvent")?.CreateDelegate(typeof(Action<object, ClientApi.Event>));
                BeforeDeleteCallback = (Action<object, ClientApi.Event>)clientTableType.GetMethod("OnBeforeDeleteEvent")?.CreateDelegate(typeof(Action<object, ClientApi.Event>));
                DeleteCallback = (Action<object, ClientApi.Event>)clientTableType.GetMethod("OnDeleteEvent")?.CreateDelegate(typeof(Action<object, ClientApi.Event>));
                UpdateCallback = (Action<object, object, ClientApi.Event>)clientTableType.GetMethod("OnUpdateEvent")?.CreateDelegate(typeof(Action<object, object, ClientApi.Event>));
                GetPrimaryKeyValueFunc = (Func<object, object>)clientTableType.GetMethod("GetPrimaryKeyValue", BindingFlags.NonPublic | BindingFlags.Static)
                    ?.CreateDelegate(typeof(Func<object, object>));
            }

            /// <summary>
            /// Decodes the given AlgebraicValue into the out parameter `obj`.
            /// </summary>
            /// <param name="value">The AlgebraicValue to decode.</param>
            /// <param name="obj">The domain object for `value`</param>
            public void SetAndForgetDecodedValue(AlgebraicValue value, out object obj)
            {
                obj = decoderFunc(value);
            }

            /// <summary>
            /// Inserts the value into the table. There can be no existing value with the provided BSATN bytes.
            /// </summary>
            /// <param name="rowBytes">The BSATN encoded bytes of the row to retrieve.</param>
            /// <param name="value">The parsed row encoded by the <paramref>rowBytes</paramref>.</param>
            /// <returns>True if the row was inserted, false if the row wasn't inserted because it was a duplicate.</returns>
            public bool InsertEntry(byte[] rowBytes, object value) => entries.TryAdd(rowBytes, value);

            /// <summary>
            /// Deletes a value from the table.
            /// </summary>
            /// <param name="rowBytes">The BSATN encoded bytes of the row to remove.</param>
            /// <returns>True if and only if the value was previously resident and has been deleted.</returns>
            public bool DeleteEntry(byte[] rowBytes)
            {
                if (entries.Remove(rowBytes))
                {
                    return true;
                }

                Logger.LogWarning("Deleting value that we don't have (no cached value available)");
                return false;
            }

            public object? GetPrimaryKeyValue(object row)
            {
                return GetPrimaryKeyValueFunc?.Invoke(row);
            }
        }

        private readonly ConcurrentDictionary<string, TableCache> tables =
            new ConcurrentDictionary<string, TableCache>();

        public void AddTable(Type clientTableType, AlgebraicType tableRowDef, Func<AlgebraicValue, object> decodeFunc)
        {
            string name = clientTableType.Name;

            if (tables.TryGetValue(name, out _))
            {
                Logger.LogError($"Table with name already exists: {name}");
                return;
            }

            // Initialize this table
            tables[name] = new TableCache(clientTableType, tableRowDef, decodeFunc);
        }

        public TableCache? GetTable(string name)
        {
            if (tables.TryGetValue(name, out var table))
            {
                return table;
            }

            Logger.LogError($"We don't know that this table is: {name}");
            return null;
        }

        public IEnumerable<object> GetObjects(string name)
        {
            return GetTable(name)?.entries.Values ?? Enumerable.Empty<object>();
        }

        public int Count(string name) => GetTable(name)?.entries.Count ?? 0;

        public IEnumerable<TableCache> GetTables() => tables.Values;
    }
}
