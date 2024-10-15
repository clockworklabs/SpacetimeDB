using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB.Internal;

namespace SpacetimeDB
{
    // TODO: merge this into `RemoteTables`.
    // It should just provide auto-generated `GetTable` and `GetTables` methods.
    public class ClientCache
    {
        private readonly IDbConnection conn;

        private readonly Dictionary<string, IRemoteTableHandle> tables = new();

        public ClientCache(IDbConnection conn) => this.conn = conn;

        public void AddTable<Row>(string name, IRemoteTableHandle table)
            where Row : IDatabaseRow, new()
        {
            if (!tables.TryAdd(name, table))
            {
                Log.Error($"Table with name already exists: {name}");
            }

            table.Initialize(name, conn);
        }

        internal IRemoteTableHandle? GetTable(string name)
        {
            if (tables.TryGetValue(name, out var table))
            {
                return table;
            }

            Log.Error($"We don't know that this table is: {name}");
            return null;
        }

        internal IEnumerable<IRemoteTableHandle> GetTables() => tables.Values;
    }
}
