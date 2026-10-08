using System;
using System.Linq;
using SpacetimeDB;
using SpacetimeDB.Internal;

#pragma warning disable STDB_UNSTABLE

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

    public static Uuid CheckReducerSelection(
        ReducerContext ctx,
        ReducerContext caller,
        int expected
    )
    {
        if (
            Module.GetInstanceId(ctx) != expected
            || ctx.Sender != caller.Sender
            || ctx.ConnectionId != caller.ConnectionId
            || ctx.Timestamp != caller.Timestamp
            || !ReferenceEquals(ctx.SenderAuth, caller.SenderAuth)
            || !ReferenceEquals(ctx.Rng, caller.Rng)
        )
        {
            throw new Exception("Selected reducer context lost caller state.");
        }
        ctx.Db.User.Insert(new User { Id = 99, Value = expected });
        if (!ctx.Db.User.Id.Delete(99))
        {
            throw new Exception("Selected reducer wrote to the wrong table.");
        }
        return ctx.NewUuidV7();
    }

    public static void Write(ProcedureTxContext ctx, int value) =>
        ctx.Db.User.Id.Update(new User { Id = 99, Value = value });

    public static void BeginSelectedTx(ProcedureTxContext ctx) =>
        ctx.Db.User.Insert(new User { Id = 99, Value = 8 });

    public static void FinishSelectedTx(ProcedureTxContext ctx, ProcedureTxContext caller)
    {
        if (
            ctx.Db.User.Id.Find(99)?.Value != 8
            || ctx.Timestamp != caller.Timestamp
            || ctx.Sender != caller.Sender
            || ctx.ConnectionId != caller.ConnectionId
            || !ReferenceEquals(ctx.SenderAuth, caller.SenderAuth)
            || !ReferenceEquals(ctx.Rng, caller.Rng)
        )
        {
            throw new Exception("Selected transaction failed to share refresh/rollback state.");
        }
        ctx.Db.User.Id.Delete(99);
    }

    public static void CheckSelectedHandlerTx(HandlerTxContext ctx, HandlerTxContext caller)
    {
        if (
            ctx.Timestamp != caller.Timestamp
            || !ReferenceEquals(ctx.Rng, caller.Rng)
            || !ReferenceEquals(ctx.SenderAuth, caller.SenderAuth)
        )
        {
            throw new Exception("Selected handler transaction copied caller state.");
        }
        ctx.Db.User.Insert(new User { Id = 99, Value = 5 });
        ctx.Db.User.Id.Delete(99);
    }

    public static void CheckProcedureSelection(
        ProcedureContext ctx,
        ProcedureContext caller,
        int expected
    )
    {
        if (
            Module.GetInstanceId(ctx) != expected
            || ctx.Sender != caller.Sender
            || ctx.ConnectionId != caller.ConnectionId
            || !ReferenceEquals(ctx.SenderAuth, caller.SenderAuth)
            || !ReferenceEquals(ctx.Rng, caller.Rng)
            || !ReferenceEquals(ctx.Http, caller.Http)
        )
        {
            throw new Exception("Selected procedure context lost caller state.");
        }
        var first = caller.NewUuidV7();
        var second = ctx.NewUuidV7();
        var third = caller.NewUuidV7();
        if (
            second.GetCounter() != first.GetCounter() + 1
            || third.GetCounter() != second.GetCounter() + 1
        )
        {
            throw new Exception("Selected procedure copied its UUID counter.");
        }
        // Exercise the base API as well as the typed transaction callback.
        ((ProcedureContextBase)ctx).WithTx(tx =>
        {
            ((ProcedureTxContext)tx).Db.User.Insert(new User { Id = 99, Value = expected });
            return true;
        });
        var failed = ctx.TryWithTx<int, InvalidOperationException>(tx =>
        {
            Write(tx, -1);
            return Result<int, InvalidOperationException>.Err(new("rollback"));
        });
        ctx.WithTx(tx =>
        {
            if (
                failed.IsSuccess
                || tx.Db.User.Id.Find(99)?.Value != expected
                || tx.Timestamp != caller.Timestamp
                || ctx.Timestamp != caller.Timestamp
            )
            {
                throw new Exception(
                    "Selected procedure transaction did not share refresh/rollback state."
                );
            }
            tx.Db.User.Id.Delete(99);
            return true;
        });
    }

    public static void CheckHandlerSelection(HandlerContext ctx, HandlerContext caller)
    {
        if (!ReferenceEquals(ctx.Rng, caller.Rng) || !ReferenceEquals(ctx.Http, caller.Http))
        {
            throw new Exception("Selected handler copied caller state.");
        }
        var first = caller.NewUuidV7();
        var second = ctx.NewUuidV7();
        var third = caller.NewUuidV7();
        if (
            second.GetCounter() != first.GetCounter() + 1
            || third.GetCounter() != second.GetCounter() + 1
        )
        {
            throw new Exception("Selected handler copied its UUID counter.");
        }
        ((HandlerContextBase)ctx).WithTx(tx =>
        {
            ((HandlerTxContext)tx).Db.User.Insert(new User { Id = 99, Value = 99 });
            return true;
        });
        var failed = ctx.TryWithTx<int, InvalidOperationException>(tx =>
        {
            tx.Db.User.Id.Update(new User { Id = 99, Value = -1 });
            return Result<int, InvalidOperationException>.Err(new("rollback"));
        });
        ctx.WithTx(tx =>
        {
            if (
                failed.IsSuccess
                || tx.Db.User.Id.Find(99)?.Value != 99
                || ctx.Timestamp != caller.Timestamp
            )
            {
                throw new Exception("Selected handler transaction lost its state.");
            }
            tx.Db.User.Id.Delete(99);
            return true;
        });
    }

    public static int Read(AnonymousViewContext ctx) => ctx.Db.User.Id.Find(1)!.Value.Value;

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
        var result = ctx.WithTx(tx =>
        {
            if (tx.Db.User.Id.Find(1)?.Value != id)
            {
                throw new Exception("Transaction rollback changed this instance.");
            }
            return tx.Db.User.Id.Find(2)!.Value.Value;
        });
        Log.Info($"next:{id}");
        return result;
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

    [View(Accessor = "QueryCurrent", Public = true)]
    public static IQuery<User> QueryCurrent(ViewContext ctx) =>
        ctx.From.User().Where(row => row.Value.Eq(Module.GetInstanceId(ctx)));

    [View(Accessor = "QueryAnonymous", Public = true)]
    public static IQuery<User> QueryAnonymous(AnonymousViewContext ctx) =>
        ctx.From.User().Where(row => row.Value.Eq(Module.GetInstanceId(ctx) + 10));

    [HttpHandler]
    public static HttpResponse Ignored(HandlerContext ctx, HttpRequest request) =>
        new(200, HttpVersion.Http11, [], HttpBody.FromString("must not be routed"));

    [HttpRouter]
    public static Router Routes() => Router.New().Get("/leaf", Handlers.Ignored);
}
