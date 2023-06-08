using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.ComponentModel.Design;
using System.Linq;
using System.Net.Http.Headers;
using System.Reflection;
using Google.Protobuf;
using UnityEngine;
using ClientApi;
using SpacetimeDB.SATS;

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
                    if (left == null || right == null)
                    {
                        return left == right;
                    }

                    return left.SequenceEqual(right);
                }

                public int GetHashCode(byte[] key)
                {
                    if (key == null)
                        throw new ArgumentNullException(nameof(key));
                    return key.Sum(b => b);
                }
            }

            private readonly string name;
            private readonly Type clientTableType;
            private readonly AlgebraicType rowSchema;

            // The function to use for decoding a type value
            private Func<AlgebraicValue, object> decoderFunc;

            // Maps from primary key to type value
            public readonly Dictionary<byte[], (AlgebraicValue, object)> entries;

            // Maps from primary key to decoded value
            public readonly ConcurrentDictionary<byte[], (AlgebraicValue, object)> decodedValues;

            public Type ClientTableType
            {
                get => clientTableType;
            }

            public Action<object, ClientApi.Event> InsertCallback;
            public Action<object, ClientApi.Event> DeleteCallback;
            public Action<object, object, ClientApi.Event> UpdateCallback;
            // TODO: Consider renaming this one, this kind of implies that its a callback for the Update operation
            public Action<NetworkManager.TableOp, object, object, ClientApi.Event> RowUpdatedCallback;
            public Func<AlgebraicType, AlgebraicValue, AlgebraicValue, bool> ComparePrimaryKeyFunc;

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
                InsertCallback = (Action<object, ClientApi.Event>)clientTableType.GetMethod("OnInsertEvent")?.CreateDelegate(typeof(Action<object, ClientApi.Event>));
                DeleteCallback = (Action<object, ClientApi.Event>)clientTableType.GetMethod("OnDeleteEvent")?.CreateDelegate(typeof(Action<object, ClientApi.Event>));
                UpdateCallback = (Action<object, object, ClientApi.Event>)clientTableType.GetMethod("OnUpdateEvent")?.CreateDelegate(typeof(Action<object, object, ClientApi.Event>));
                RowUpdatedCallback = (Action<NetworkManager.TableOp, object, object, ClientApi.Event>)clientTableType.GetMethod("OnRowUpdateEvent")
                    ?.CreateDelegate(typeof(Action<NetworkManager.TableOp, object, object, ClientApi.Event>));
                ComparePrimaryKeyFunc = (Func<AlgebraicType, AlgebraicValue, AlgebraicValue, bool>)clientTableType.GetMethod("ComparePrimaryKey", BindingFlags.Static | BindingFlags.Public)
                    ?.CreateDelegate(typeof(Func<AlgebraicType, AlgebraicValue, AlgebraicValue, bool>));
                entries = new Dictionary<byte[], (AlgebraicValue, object)>(new ByteArrayComparer());
                decodedValues = new ConcurrentDictionary<byte[], (AlgebraicValue, object)>(new ByteArrayComparer());
            }

            public bool GetDecodedValue(byte[] pk, out AlgebraicValue value, out object obj)
            {
                if (decodedValues.TryGetValue(pk, out var decoded))
                {
                    value = decoded.Item1;
                    obj = decoded.Item2;
                    return true;
                }

                value = null;
                obj = null;
                return false;
            }

            /// <summary>
            /// Decodes the given AlgebraicValue into the out parameter `obj`.
            /// </summary>
            /// <param name="pk">The primary key of the row associated with `value`.</param>
            /// <param name="value">The AlgebraicValue to decode.</param>
            /// <param name="obj">The domain object for `value`</param>
            public void SetDecodedValue(byte[] pk, AlgebraicValue value, out object obj)
            {
                if (decodedValues.TryGetValue(pk, out var existingObj))
                {
                    obj = existingObj.Item2;
                }
                else
                {
                    var decoded = (value, decoderFunc(value));
                    decodedValues[pk] = decoded;
                    obj = decoded.Item2;
                }
            }

            /// <summary>
            /// Inserts the value into the table. There can be no existing value with the provided pk.
            /// </summary>
            /// <returns></returns>
            public object InsertEntry(byte[] rowPk)
            {
                if (entries.TryGetValue(rowPk, out var existingValue))
                {
                    // Debug.LogWarning($"We tried to insert a database row that already exists. table={Name} RowPK={Convert.ToBase64String(rowPk)}");
                    return existingValue.Item2;
                }

                if (GetDecodedValue(rowPk, out var value, out var obj))
                {
                    entries[rowPk] = (value, obj);
                    return obj;
                }

                // Read failure
                Debug.LogError(
                    $"Read error when converting row value for table: {clientTableType.Name} rowPk={Convert.ToBase64String(rowPk)} (version issue?)");
                return null;
            }

            /// <summary>
            /// Updates an entry. Returns whether or not the update was successful. Updates only succeed if
            /// a previous value was overwritten.
            /// </summary>
            /// <param name="pk">The primary key that uniquely identifies this row</param>
            /// <param name="newValueByteString">The new for the table entry</param>
            /// <returns>True when the old value was removed and the new value was inserted.</returns>
            public bool UpdateEntry(ByteString pk, ByteString newValueByteString)
            {
                // We have to figure out if pk is going to change or not
                throw new InvalidOperationException();
            }

            /// <summary>
            /// Deletes a value from the table.
            /// </summary>
            /// <param name="rowPk">The primary key that uniquely identifies this row</param>
            /// <returns></returns>
            public object DeleteEntry(byte[] rowPk)
            {
                if (entries.TryGetValue(rowPk, out var value))
                {
                    entries.Remove(rowPk);
                    return value.Item2;
                }

                // SpacetimeDB is asking us to delete something we don't have, this makes no sense. We can
                // fabricate the deletion by trying to look it up in our local decode table.
                if (decodedValues.TryGetValue(rowPk, out var decodedValue))
                {
                    Debug.LogWarning("Deleting value that we don't have (using cached value)");
                    return decodedValue.Item2;
                }

                Debug.LogWarning("Deleting value that we don't have (no cached value available)");
                return null;
            }

            public bool ComparePrimaryKey(AlgebraicValue v1, AlgebraicValue v2)
            {
                return (bool)ComparePrimaryKeyFunc.Invoke(rowSchema, v1, v2);
            }
            
            public bool ComparePrimaryKey(byte[] rowPk1, byte[] rowPk2)
            {
                if (!decodedValues.TryGetValue(rowPk1, out var v1))
                {
                    return false;
                }
                if (!decodedValues.TryGetValue(rowPk2, out var v2))
                {
                    return false;
                }
                
                return (bool)ComparePrimaryKeyFunc.Invoke(rowSchema, v1.Item1, v2.Item1);
            }
        }

        private readonly ConcurrentDictionary<string, TableCache> tables =
            new ConcurrentDictionary<string, TableCache>();

        public void AddTable(Type clientTableType, AlgebraicType tableRowDef, Func<AlgebraicValue, object> decodeFunc)
        {
            string name = clientTableType.Name;

            if (tables.TryGetValue(name, out _))
            {
                Debug.LogError($"Table with name already exists: {name}");
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

            Debug.LogError($"We don't know that this table is: {name}");
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
    }
}
