using System;
using System.Linq;
using SpacetimeDB;
using SpacetimeDB.Internal;

namespace NestedLeaf;

[Table(Accessor = "User", Public = true)]
[SpacetimeDB.Index.BTree(Accessor = "ByValue", Columns = [nameof(Value)])]
public partial struct User
{
    [PrimaryKey]
    public int Id;
    public int Value;
}

[Table(Accessor = "RefUser", Public = true)]
public partial class RefUser
{
    [PrimaryKey]
    public int Id;
    public int Value;
}

public static partial class Functions
{
    public static ulong Count(ReducerContext ctx) => ctx.Db.User.Count;

    [Reducer]
    public static void Ping(ReducerContext ctx)
    {
        var id = Module.GetInstanceId(ctx);
        var table = ctx.Db.User;
        if (!ReferenceEquals(table.Id, ctx.Db.User.Id))
        {
            throw new Exception("Table and index handles must be cached.");
        }
        if (table.Count != 0)
        {
            throw new Exception("Another instance wrote into this table.");
        }
        table.Insert(new User { Id = 1, Value = id });
        table.Id.Update(new User { Id = 1, Value = id + 100 });
        if (table.ByValue.Filter(id + 100).Single().Id != 1)
        {
            throw new Exception("Index update targeted the wrong instance.");
        }
        ctx.Db.RefUser.Insert(new RefUser { Id = 1, Value = id });
        ctx.Db.RefUser.Id.Update(new RefUser { Id = 1, Value = id + 100 });
        if (ctx.Db.RefUser.Id.Find(1)?.Value != id + 100)
        {
            throw new Exception("Reference-row update targeted the wrong instance.");
        }
        Log.Info($"leaf:{id}");
    }

    [Reducer]
    public static void Pong(ReducerContext ctx)
    {
        var table = ctx.Db.User;
        var id = Module.GetInstanceId(ctx);
        if (table.Iter().Single().Value != id + 100 || table.ByValue.Delete(id + 100) != 1)
        {
            throw new Exception("Index scan/delete targeted the wrong instance.");
        }
        table.Insert(new User { Id = 2, Value = id });
        if (!table.Delete(new User { Id = 2, Value = id }))
        {
            throw new Exception("Row delete failed.");
        }
        table.Insert(new User { Id = 3, Value = id });
        table.Clear();
        if (table.Count != 0)
        {
            throw new Exception("Clear targeted the wrong instance.");
        }
        if (!ctx.Db.RefUser.Id.Delete(1) || ctx.Db.RefUser.Count != 0)
        {
            throw new Exception("Unique-index delete targeted the wrong instance.");
        }
        Log.Info($"pong:{id}");
    }

    [Procedure]
    public static int Instance(ProcedureContext ctx) => Module.GetInstanceId(ctx);

    [Procedure]
    public static int Next(ProcedureContext ctx)
    {
        var id = Module.GetInstanceId(ctx);
        ctx.WithTx(tx =>
        {
            tx.Db.User.Insert(new User { Id = 1, Value = id });
            tx.Db.User.Insert(new User { Id = 2, Value = id + 10 });
            tx.Db.RefUser.Insert(new RefUser { Id = 1, Value = id });
            return true;
        });
        try
        {
            ctx.WithTx<bool>(tx =>
            {
                tx.Db.User.Id.Update(new User { Id = 1, Value = -1 });
                throw new InvalidOperationException("rollback");
            });
        }
        catch (InvalidOperationException error) when (error.Message == "rollback") { }
        return ctx.WithTx(tx =>
        {
            if (tx.Db.User.Id.Find(1)?.Value != id)
            {
                throw new Exception("Transaction rollback changed this instance.");
            }
            return tx.Db.User.Id.Find(2)!.Value.Value;
        });
    }

    [View(Accessor = "Current", Public = true)]
    public static User? Current(ViewContext ctx)
    {
        var row = ctx.Db.User.Id.Find(1);
        if (ctx.Db.RefUser.Id.Find(1)?.Value != row?.Value)
        {
            throw new Exception("Read-only reference-row index targeted the wrong instance.");
        }
        return row;
    }

    [View(Accessor = "Anonymous", Public = true)]
    public static User? Anonymous(AnonymousViewContext ctx) =>
        ctx.Db.User.ByValue.Filter(Module.GetInstanceId(ctx) + 10).Single();

    [HttpHandler]
    public static HttpResponse Ignored(HandlerContext ctx, HttpRequest request) =>
        new(200, HttpVersion.Http11, [], HttpBody.FromString("must not be routed"));

    [HttpRouter]
    public static Router Routes() => Router.New().Get("/leaf", Handlers.Ignored);
}
