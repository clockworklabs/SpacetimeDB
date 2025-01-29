using System.Collections.Generic;

namespace SpacetimeDB
{
    // TODO: merge this into `RemoteTables`.
    // It should just provide auto-generated `GetTable` and `GetTables` methods.
    public sealed class ClientCache
    {
        private readonly IDbConnection conn;

        private readonly Dictionary<string, IRemoteTableHandle> tables = new();

        public ClientCache(IDbConnection conn) => this.conn = conn;

        public void AddTable(IRemoteTableHandle table)
        {
            tables.Add(table.Name, table);
            table.Initialize(conn);
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
