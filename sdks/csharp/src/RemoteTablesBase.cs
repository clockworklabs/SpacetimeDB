using System.Collections.Generic;

namespace SpacetimeDB
{
    public abstract class RemoteTablesBase
    {
        private readonly Dictionary<string, IRemoteTableHandle> tables = new();

        protected void AddTable(IRemoteTableHandle table)
        {
            tables.Add(table.RemoteTableName, table);
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
    }
}
