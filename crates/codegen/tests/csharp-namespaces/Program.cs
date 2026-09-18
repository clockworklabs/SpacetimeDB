using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using Game.Bindings;
using SpacetimeDB;
using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;
using Auth = Game.Bindings.MyAuth;
using Audit = Game.Bindings.@class;

internal static class Program
{
    private static void Equal<T>(T expected, T actual)
    {
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
            throw new Exception($"Expected {expected}, got {actual}");
    }

    private static BsatnRowList Rows(params IStructuralReadWrite[] rows)
    {
        using var stream = new MemoryStream();
        using var writer = new BinaryWriter(stream);
        var offsets = new List<ulong>();
        foreach (var row in rows)
        {
            offsets.Add((ulong)stream.Length);
            row.WriteFields(writer);
        }
        return new BsatnRowList(new RowSizeHint.RowOffsets(offsets), stream.ToArray().ToList());
    }

    private static TableUpdate Change(string name, IStructuralReadWrite[] inserts, IStructuralReadWrite[] deletes) =>
        new TableUpdate(name, new List<TableUpdateRows>
        {
            new TableUpdateRows.PersistentTable(new PersistentTableRows(Rows(inserts), Rows(deletes)))
        });

    private static void Apply(DbConnection conn, params TableUpdate[] updates)
    {
        var parsed = ParsedDatabaseUpdate.New();
        foreach (var update in updates)
            conn.Db.GetTable(update.TableName)!.Parse(update, parsed);
        var ctx = new EventContext(conn, new Event<Reducer>.UnknownTransaction());
        foreach (var (table, delta) in parsed.Updates) table.PreApply(ctx, delta);
        foreach (var (table, delta) in parsed.Updates) table.Apply(ctx, delta);
        foreach (var (table, _) in parsed.Updates) table.PostApply(ctx);
    }

    private static void Main()
    {
        var conn = new DbConnection();
        try
        {
            Check(conn);
        }
        finally
        {
            conn.Disconnect();
        }
    }

    private static void Check(DbConnection conn)
    {
        Equal("user", conn.Db.User.RemoteTableName);
        Equal("MyAuth.user", conn.Db.MyAuth.User.RemoteTableName);
        Equal("class.user", conn.Db.@class.User.RemoteTableName);
        Equal<IRemoteTableHandle>(conn.Db.MyAuth.User, conn.Db.GetTable("MyAuth.user")!);
        Equal<IRemoteTableHandle>(conn.Db.@class.User, conn.Db.GetTable("class.user")!);

        var query = new QueryBuilder();
        Equal("SELECT * FROM \"user\"", query.From.User().ToSql());
        Equal("SELECT * FROM \"MyAuth\".\"user\"", query.From.MyAuth.User().ToSql());
        Equal("SELECT * FROM \"class\".\"user\"", query.From.@class.User().ToSql());
        var filtered = query.From.MyAuth.User().Where(cols => cols.Value.Eq(7U)).ToSql();
        if (!filtered.Contains("\"MyAuth\".\"user\".\"value\"")) throw new Exception(filtered);
        var join = " FROM \"MyAuth\".\"user\" JOIN \"class\".\"user\" ON \"MyAuth\".\"user\".\"id\" = \"class\".\"user\".\"id\"";
        Equal("SELECT \"MyAuth\".\"user\".*" + join, query.From.MyAuth.User().LeftSemijoin(query.From.@class.User(), (l, r) => l.Id.Eq(r.Id)).ToSql());
        Equal("SELECT \"class\".\"user\".*" + join, query.From.MyAuth.User().RightSemijoin(query.From.@class.User(), (l, r) => l.Id.Eq(r.Id)).ToSql());
        var all = QueryBuilder.AllTablesSqlQueries();
        Equal(9, all.Length);
        Equal(all.Length, all.Distinct().Count());
        Equal(false, all.Any(sql => sql.Contains("secret")));
        var sqlName = typeof(Auth.RemoteTables.UserHandle).GetProperty("RemoteSqlTableName", BindingFlags.Instance | BindingFlags.NonPublic)!;
        Equal("\"MyAuth\".\"user\"", sqlName.GetValue(conn.Db.MyAuth.User)!.ToString());

        var root = new User(1, true);
        var auth = new Auth.User(1, 7);
        var audit = new Audit.User(1, "audit");
        int inserts = 0;
        conn.Db.MyAuth.User.OnInsert += (ctx, row) =>
        {
            Equal(conn.Db, ctx.Db);
            Equal(1, ctx.Db.User.Count);
            Equal(1, ctx.Db.@class.User.Count);
            Equal(7U, row.Value);
            inserts++;
        };
        Apply(conn,
            Change("user", new[] { root }, Array.Empty<User>()),
            Change("MyAuth.user", new[] { auth }, Array.Empty<Auth.User>()),
            Change("class.user", new[] { audit }, Array.Empty<Audit.User>()));
        Equal(1, inserts);
        Equal(auth, conn.Db.MyAuth.User.Id.Find(1));
        Equal(audit, conn.Db.@class.User.Id.Find(1));
        Equal("audit", conn.Db.@class.User.Iter().Single().Value);
        int updates = 0;
        conn.Db.MyAuth.User.OnUpdate += (ctx, oldRow, newRow) =>
        {
            Equal(7U, oldRow.Value);
            Equal(8U, newRow.Value);
            Equal("audit", ctx.Db.@class.User.Id.Find(1)!.Value);
            updates++;
        };
        var updatedAuth = new Auth.User(1, 8);
        Apply(conn, Change("MyAuth.user", new[] { updatedAuth }, new[] { auth }));
        Equal(1, updates);
        auth = updatedAuth;
        Apply(conn, Change("MyAuth.user", Array.Empty<Auth.User>(), new[] { auth }));
        Equal(0, conn.Db.MyAuth.User.Count);
        Equal(1, conn.Db.User.Count);
        Equal(1, conn.Db.@class.User.Count);

        int notices = 0;
        conn.Db.MyAuth.Notice.OnInsert += (ctx, row) => { Equal(4UL, row.Id); notices++; };
        Apply(conn, new TableUpdate("MyAuth.notice", new List<TableUpdateRows>
        {
            new TableUpdateRows.EventTable(new EventTableRows(Rows(new Auth.Notice(4))))
        }));
        Equal(1, notices);

        Equal("MyAuth.login", ((IReducerArgs)new Auth.Reducer.Login(auth)).ReducerName);
        Equal("class.login", ((IReducerArgs)new Audit.Reducer.Login(audit)).ReducerName);
        Equal("MyAuth.get_user", ((IProcedureArgs)new Auth.Procedure.GetUserArgs()).ProcedureName);
        // Function-only types are emitted and remain in their owning typespace.
        _ = new Auth.Procedure.GetPayload { Value = new Auth.Payload("auth") };
        _ = new Audit.Procedure.GetUser { Value = audit };
        _ = new Auth.Procedure.GetUser { Value = auth };

        int authCalls = 0, auditCalls = 0, errors = 0;
        conn.Reducers.MyAuth.OnLogin += (ctx, row) => { Equal(auth, row); authCalls++; };
        void OnAuditLogin(ReducerEventContext ctx, Audit.User row) { Equal(audit, row); auditCalls++; }
        conn.Reducers.@class.OnLogin += OnAuditLogin;
        conn.OnUnhandledReducerError += (ctx, error) => { Equal("failed", error.Message); errors++; };
        var dispatch = typeof(DbConnection).GetMethod("Dispatch", BindingFlags.NonPublic | BindingFlags.Instance)!;
        void Dispatch(Reducer args, Status status)
        {
            var ev = new ReducerEvent<Reducer>(new Timestamp(0), status, Identity.From(new byte[32]), null, null, args);
            dispatch.Invoke(conn, new object[] { new ReducerEventContext(conn, ev), args });
        }
        Dispatch(new Auth.Reducer.Login(auth), new Status.Committed(new Unit()));
        Dispatch(new Audit.Reducer.Login(audit), new Status.Committed(new Unit()));
        conn.Reducers.@class.OnLogin -= OnAuditLogin;
        Dispatch(new Audit.Reducer.Login(audit), new Status.Failed("failed"));
        Equal(1, authCalls);
        Equal(1, auditCalls);
        Equal(1, errors);
        Console.WriteLine("Namespace client checks passed.");
    }
}
