using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Reflection;
using SpacetimeDB.SATS;
using System.Numerics;
using System.Runtime.CompilerServices;

namespace SpacetimeDB
{
    public class ClientCache
    {
        public class TableCache
        {
            public class ByteArrayComparer : IEqualityComparer<byte[]>
            {
                public bool Equals(byte[] left, byte[] right)
                {
                    if (ReferenceEquals(left, right))
                    {
                        return true;
                    }

                    if (left == null || right == null || left.Length != right.Length)
                    {
                        return false;
                    }

                    return EqualsUnvectorized(left, right);

                }

                [MethodImpl(MethodImplOptions.AggressiveInlining)]
                private bool EqualsUnvectorized(byte[] left, byte[] right)
                {
                    for (int i = 0; i < left.Length; i++)
                    {
                        if (left[i] != right[i])
                        {
                            return false;
                        }
                    }

                    return true;
                }

                public int GetHashCode(byte[] obj)
                {
                    int hash = 17;
                    foreach (byte b in obj)
                    {
                        hash = hash * 31 + b;
                    }
                    return hash;
                }
            }

            private readonly string name;
            private readonly Type clientTableType;
            private readonly AlgebraicType rowSchema;

            // The function to use for decoding a type value
            private Func<AlgebraicValue, object> decoderFunc;

            // Maps from primary key to type value
            public readonly Dictionary<byte[], (AlgebraicValue, object)> entries;

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
            // TODO: Consider renaming this one, this kind of implies that its a callback for the Update operation
            public Action<SpacetimeDBClient.TableOp, object, object, ClientApi.Event> RowUpdatedCallback;
            public Func<AlgebraicType, AlgebraicValue, AlgebraicValue, bool> ComparePrimaryKeyFunc;
            public Func<AlgebraicValue, AlgebraicValue> GetPrimaryKeyValueFunc;
            public Func<AlgebraicType, AlgebraicType> GetPrimaryKeyTypeFunc;

            public string Name
            {
                get => name;
            }

            public AlgebraicType RowSchema
            {
                get => rowSchema;
            }

            public TableCache(Type clientTableType, AlgebraicType rowSchema, Func<AlgebraicValue, object> decoderFunc)
            {
                name = clientTableType.Name;
                this.clientTableType = clientTableType;

                this.rowSchema = rowSchema;
                this.decoderFunc = decoderFunc;
                InternalValueInsertedCallback = (Action<object>)clientTableType.GetMethod("InternalOnValueInserted", BindingFlags.NonPublic | BindingFlags.Static)?.CreateDelegate(typeof(Action<object>));
                InternalValueDeletedCallback = (Action<object>)clientTableType.GetMethod("InternalOnValueDeleted", BindingFlags.NonPublic | BindingFlags.Static)?.CreateDelegate(typeof(Action<object>));
                InsertCallback = (Action<object, ClientApi.Event>)clientTableType.GetMethod("OnInsertEvent")?.CreateDelegate(typeof(Action<object, ClientApi.Event>));
                BeforeDeleteCallback = (Action<object, ClientApi.Event>)clientTableType.GetMethod("OnBeforeDeleteEvent")?.CreateDelegate(typeof(Action<object, ClientApi.Event>));
                DeleteCallback = (Action<object, ClientApi.Event>)clientTableType.GetMethod("OnDeleteEvent")?.CreateDelegate(typeof(Action<object, ClientApi.Event>));
                UpdateCallback = (Action<object, object, ClientApi.Event>)clientTableType.GetMethod("OnUpdateEvent")?.CreateDelegate(typeof(Action<object, object, ClientApi.Event>));
                RowUpdatedCallback = (Action<SpacetimeDBClient.TableOp, object, object, ClientApi.Event>)clientTableType.GetMethod("OnRowUpdateEvent")
                    ?.CreateDelegate(typeof(Action<SpacetimeDBClient.TableOp, object, object, ClientApi.Event>));
                ComparePrimaryKeyFunc = (Func<AlgebraicType, AlgebraicValue, AlgebraicValue, bool>)clientTableType.GetMethod("ComparePrimaryKey", BindingFlags.Static | BindingFlags.Public)
                    ?.CreateDelegate(typeof(Func<AlgebraicType, AlgebraicValue, AlgebraicValue, bool>));
                GetPrimaryKeyValueFunc = (Func<AlgebraicValue, AlgebraicValue>)clientTableType.GetMethod("GetPrimaryKeyValue", BindingFlags.Static | BindingFlags.Public)
                    ?.CreateDelegate(typeof(Func<AlgebraicValue, AlgebraicValue>));
                GetPrimaryKeyTypeFunc = (Func<AlgebraicType, AlgebraicType>)clientTableType.GetMethod("GetPrimaryKeyType", BindingFlags.Static | BindingFlags.Public)
                    ?.CreateDelegate(typeof(Func<AlgebraicType, AlgebraicType>));
                entries = new Dictionary<byte[], (AlgebraicValue, object)>(new ByteArrayComparer());
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
            /// Inserts the value into the table. There can be no existing value with the provided pk.
            /// </summary>
            /// <returns>True if the row was inserted, false if the row wasn't inserted because it was a duplicate.</returns>
            public bool InsertEntry(byte[] rowPk, AlgebraicValue value)
            {
                if (entries.ContainsKey(rowPk))
                {
                    return false;
                }
               
                // Insert the row into our table
                entries[rowPk] = (value, decoderFunc(value));
                return true;
            }

            /// <summary>
            /// Deletes a value from the table.
            /// </summary>
            /// <param name="rowPk">The primary key that uniquely identifies this row</param>
            /// <returns></returns>
            public bool DeleteEntry(byte[] rowPk)
            {
                if (entries.TryGetValue(rowPk, out var value))
                {
                    entries.Remove(rowPk);
                    return true;
                }

                SpacetimeDBClient.instance.Logger.LogWarning("Deleting value that we don't have (no cached value available)");
                return false;
            }

            /// <summary>
            /// Gets a value from the table
            /// </summary>
            /// <param name="rowPk">The primary key that uniquely identifies this row</param>
            /// <returns></returns>
            public bool TryGetValue(byte[] rowPk, out object value)
            {
                if (entries.TryGetValue(rowPk, out var v))
                {
                    value = v.Item2;
                    return true;
                }

                value = null;
                return false;
            }

            public bool ComparePrimaryKey(AlgebraicValue v1, AlgebraicValue v2)
            {
                return (bool)ComparePrimaryKeyFunc.Invoke(rowSchema, v1, v2);
            }

            public AlgebraicValue GetPrimaryKeyValue(AlgebraicValue row)
            {
                return GetPrimaryKeyValueFunc != null ? GetPrimaryKeyValueFunc.Invoke(row) : null;
            }

            public AlgebraicType GetPrimaryKeyType()
            {
                return GetPrimaryKeyTypeFunc != null ? GetPrimaryKeyTypeFunc.Invoke(rowSchema) : null;
            }
        }

        private readonly ConcurrentDictionary<string, TableCache> tables =
            new ConcurrentDictionary<string, TableCache>();

        public void AddTable(Type clientTableType, AlgebraicType tableRowDef, Func<AlgebraicValue, object> decodeFunc)
        {
            string name = clientTableType.Name;

            if (tables.TryGetValue(name, out _))
            {
                SpacetimeDBClient.instance.Logger.LogError($"Table with name already exists: {name}");
                return;
            }

            // Initialize this table
            tables[name] = new TableCache(clientTableType, tableRowDef, decodeFunc);
        }

        public IEnumerable<object> GetObjects(string name)
        {
            if (!tables.TryGetValue(name, out var table))
            {
                yield break;
            }

            foreach (var entry in table.entries)
            {
                yield return entry.Value.Item2;
            }
        }

        public IEnumerable<(AlgebraicValue, object)> GetEntries(string name)
        {
            if (!tables.TryGetValue(name, out var table))
            {
                yield break;
            }

            foreach (var entry in table.entries)
            {
                yield return entry.Value;
            }
        }

        public TableCache GetTable(string name)
        {
            if (tables.TryGetValue(name, out var table))
            {
                return table;
            }

            SpacetimeDBClient.instance.Logger.LogError($"We don't know that this table is: {name}");
            return null;
        }

        public int Count(string name)
        {
            if (!tables.TryGetValue(name, out var table))
            {
                return 0;
            }

            return table.entries.Count;
        }

        public IEnumerable<string> GetTableNames() => tables.Keys;
        
        public IEnumerable<TableCache> GetTables() => tables.Values;
    }
}
