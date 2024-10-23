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

        private readonly Dictionary<uint, IRemoteTableHandle> tables = new();

        public ClientCache(IDbConnection conn) => this.conn = conn;

        public void AddTable<Row>(uint tableIdx, IRemoteTableHandle table)
            where Row : IDatabaseRow, new()
        {
            if (!tables.TryAdd(tableIdx, table))
            {
                Log.Error($"Table with index already exists: {tableIdx}");
            }

            table.Initialize(tableIdx, conn);
        }

        internal IRemoteTableHandle? GetTable(uint tableIdx)
        {
            if (tables.TryGetValue(tableIdx, out var table))
            {
                return table;
            }

            Log.Error($"We don't know that this table is: {tableIdx}");
            return null;
        }

        internal IEnumerable<IRemoteTableHandle> GetTables() => tables.Values;
    }
}
