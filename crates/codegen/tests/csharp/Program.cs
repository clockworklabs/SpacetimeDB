using System;
using SpacetimeDB;

namespace SpacetimeDB
{
    public sealed partial class Person
    {
        public string DisplayName() => $"{Name} ({Id})";
    }
}

internal static class Program
{
    private static void Main()
    {
        Func<DbConnectionBuilder<DbConnection>> builder = DbConnection.Builder;
        var conn = new DbConnection();
        try
        {
            var row = new Person(Id: 1, Name: "Alice", Age: 42);
            if (row.DisplayName() != "Alice (1)" || new Person().Name != "")
                throw new Exception("row constructors or partial extension changed");

            RemoteTables.PersonHandle table = conn.Db.Person;
            RemoteTables.PersonHandle.IdUniqueIndex id = table.Id;
            if (id.Find(row.Id) != null || table.RemoteTableName != "person")
                throw new Exception("unexpected table or primary key lookup");

            RemoteReducers.AddHandler handler = (ctx, name, age) => { };
            conn.Reducers.OnAdd += handler;
            conn.Reducers.OnAdd -= handler;
            var args = new Reducer.Add(Name: "Alice", Age: 42);
            if (((IReducerArgs)args).ReducerName != "add")
                throw new Exception("reducer name changed");
            Action<string, byte> add = conn.Reducers.Add;
            Action<ulong, ProcedureCallback<Baz>> read = conn.Procedures.ReturnValue;

            var query = new QueryBuilder().From.Person().Where(p => p.Age.Gt((byte)18));
            if (!query.ToSql().Contains("\"person\""))
                throw new Exception(query.ToSql());
            TypedSubscriptionBuilder subscription = conn.SubscriptionBuilder()
                .AddQuery(q => q.From.MyPlayer());
        }
        finally
        {
            conn.Disconnect();
        }
    }
}
