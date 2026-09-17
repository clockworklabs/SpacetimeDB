using System;
using System.Diagnostics;
using System.Linq;
using System.Threading;
using SpacetimeDB;
using SpacetimeDB.Types;

internal static class Program
{
    private static void Require(bool condition, string message)
    {
        if (!condition)
            throw new Exception(message);
    }

    private static void Main()
    {
        var host =
            Environment.GetEnvironmentVariable("SPACETIMEDB_SERVER_URL") ?? "http://localhost:3000";
        if (host == "local")
            host = "http://localhost:3000";
        var connected = false;
        Exception? connectionError = null;
        var conn = DbConnection
            .Builder()
            .WithUri(host)
            .WithDatabaseName("namespace-tests")
            .OnConnect((_, _, _) => connected = true)
            .OnConnectError(error => connectionError = error)
            .OnDisconnect(
                (_, error) => connectionError = error ?? new Exception("Unexpected disconnect")
            )
            .Build();

        void Wait(Func<bool> done, string phase)
        {
            var timer = Stopwatch.StartNew();
            while (!done())
            {
                if (connectionError != null)
                    throw connectionError;
                if (timer.Elapsed > TimeSpan.FromSeconds(30))
                    throw new TimeoutException(phase);
                conn.FrameTick();
                Thread.Sleep(5);
            }
            if (connectionError != null)
                throw connectionError;
        }

        void Unsubscribe(SubscriptionHandle handle)
        {
            var done = false;
            handle.UnsubscribeThen(_ => done = true);
            Wait(() => done, "unsubscribe");
        }

        void EmptyCache()
        {
            Require(
                conn.Db.User.Count == 0
                    && conn.Db.MyAuth.User.Count == 0
                    && conn.Db.@class.User.Count == 0
                    && conn.Db.ExtraRow.Count == 0,
                "Unsubscribe must clear every namespace's table cache"
            );
            Require(
                conn.Db.Users.Count == 0
                    && conn.Db.MyAuth.Users.Count == 0
                    && conn.Db.MyAuth.QueryUsers.Count == 0
                    && conn.Db.MyAuth.AnonymousUsers.Count == 0
                    && conn.Db.AuthUsers.Count == 0
                    && conn.Db.QueryUsers.Count == 0
                    && conn.Db.QueryUsersRight.Count == 0
                    && conn.Db.QueryExtra.Count == 0,
                "Unsubscribe must clear view caches"
            );
        }

        try
        {
            Wait(() => connected, "connect");
            Require(
                conn.Db.MyAuth.User.RemoteTableName == "MyAuth.auth_users",
                "Explicit table wire name"
            );
            Require(
                conn.Db.@class.User.RemoteTableName == "class.user",
                "Keyword namespace wire name"
            );
            Require(
                !QueryBuilder.AllTablesSqlQueries().Any(sql => sql.Contains("secret")),
                "Subscribe-all must omit private child tables"
            );

            var applied = false;
            var subscription = conn.SubscriptionBuilder()
                .OnApplied(_ => applied = true)
                .OnError((_, error) => throw error)
                .AddQuery(q => q.From.User())
                .AddQuery(q => q.From.MyAuth.User())
                .AddQuery(q => q.From.@class.User())
                .AddQuery(q => q.From.ExtraRow())
                .AddQuery(q => q.From.Users())
                .AddQuery(q => q.From.MyAuth.Users())
                .AddQuery(q => q.From.MyAuth.AnonymousUsers())
                .AddQuery(q => q.From.MyAuth.QueryUsers())
                .AddQuery(q => q.From.MyAuth.Notice())
                .Subscribe();
            Wait(() => applied, "typed subscription");
            EmptyCache();

            conn.Reducers.Extra();
            Wait(() => conn.Db.ExtraRow.Count == 1, "dependency automatically registered in public");
            var exerciseDone = false;
            var atomicInsert = false;
            conn.Db.User.OnInsert += (ctx, row) =>
            {
                if (row.Id != 2)
                    return;
                Require(
                    ctx.Db.MyAuth.User.Id.Find(2)?.Score == 99
                        && ctx.Db.@class.User.Id.Find(4)?.Message == "consumer",
                    "Root callback must see the complete cross-namespace transaction"
                );
                atomicInsert = true;
            };
            conn.Reducers.OnExercise += (ctx) =>
            {
                Require(ctx.Event.Status is Status.Committed, "Exercise failed");
                exerciseDone = true;
            };
            conn.Reducers.Exercise();
            Wait(
                () =>
                    exerciseDone
                    && atomicInsert
                    && conn.Db.MyAuth.Users.Count == 1
                    && conn.Db.MyAuth.AnonymousUsers.Count == 1
                    && conn.Db.MyAuth.QueryUsers.Count == 1,
                "cross-library helper writes and views"
            );
            Require(
                conn.Db.User.Count == 1
                    && conn.Db.MyAuth.User.Count == 1
                    && conn.Db.@class.User.Count == 1,
                "Same-named table caches must be independent"
            );

            var joinApplied = false;
            var joins = conn.SubscriptionBuilder()
                .OnApplied(_ => joinApplied = true)
                .OnError((_, error) => throw error)
                .AddQuery(q =>
                    q.From.User().LeftSemijoin(q.From.MyAuth.User(), (r, a) => r.Id.Eq(a.Id))
                )
                .AddQuery(q =>
                    q.From.User().RightSemijoin(q.From.MyAuth.User(), (r, a) => r.Id.Eq(a.Id))
                )
                .AddQuery(q => q.From.MyAuth.User().Where(c => c.Score.Eq(99u)))
                .Subscribe();
            Wait(() => joinApplied, "cross-namespace semijoins and filter");
            Require(
                conn.Db.User.Count == 1 && conn.Db.MyAuth.User.Count == 1,
                "Overlapping queries duplicate rows"
            );
            Unsubscribe(joins);
            Require(
                conn.Db.User.Count == 1 && conn.Db.MyAuth.User.Count == 1,
                "Overlapping unsubscribe removed live rows"
            );

            var authAdded = false;
            var auditAdded = false;
            var notices = 0;
            conn.Db.MyAuth.Notice.OnInsert += (_, row) =>
            {
                Require(row.Id == 10, "Event routed to wrong namespace");
                notices++;
            };
            conn.Reducers.MyAuth.OnAdd += (ctx, id) =>
            {
                Require(ctx.Event.Status is Status.Committed && id == 10, "Auth reducer result");
                Require(ctx.Db.MyAuth.User.Id.Find(id)?.Score == 42, "Auth reducer callback cache");
                authAdded = true;
            };
            conn.Reducers.@class.OnAdd += (ctx, id) =>
            {
                Require(ctx.Event.Status is Status.Committed && id == 10, "Audit reducer result");
                Require(
                    ctx.Db.@class.User.Id.Find(id)?.Message == "audit",
                    "Audit reducer callback cache"
                );
                auditAdded = true;
            };
            conn.Reducers.MyAuth.Add(10);
            conn.Reducers.@class.Add(10);
            Wait(
                () => authAdded && auditAdded && notices == 1,
                "same-named reducers and event table"
            );
            Require(
                conn.Db.User.Count == 1
                    && conn.Db.MyAuth.User.Count == 2
                    && conn.Db.@class.User.Count == 2,
                "Namespaced reducers changed the wrong table"
            );

            var procedures = 0;
            conn.Procedures.CountUsers(
                (_, result) =>
                {
                    Require(result.IsSuccess && result.Value == 5, "Root procedure");
                    procedures++;
                }
            );
            conn.Procedures.MyAuth.CountUsers(
                (_, result) =>
                {
                    Require(result.IsSuccess && result.Value == 2, "Auth procedure");
                    procedures++;
                }
            );
            conn.Procedures.@class.CountUsers(
                (_, result) =>
                {
                    Require(result.IsSuccess && result.Value == 2, "Audit procedure");
                    procedures++;
                }
            );
            conn.Procedures.MyAuth.ReadScore(
                10,
                (_, result) =>
                {
                    Require(result.IsSuccess && result.Value == 42, "Procedure return value");
                    procedures++;
                }
            );
            conn.Procedures.MyAuth.ReadScore(
                999,
                (_, result) =>
                {
                    Require(
                        !result.IsSuccess && result.Error != null,
                        "Procedure failure callback"
                    );
                    procedures++;
                }
            );
            Wait(() => procedures == 5, "namespaced procedure callbacks");

            var updated = false;
            conn.Db.MyAuth.User.OnUpdate += (ctx, before, after) =>
            {
                Require(
                    before.Id == 10 && before.Score == 42 && after.Score == 77,
                    "Update callback values"
                );
                Require(
                    ctx.Db.MyAuth.User.ByScore.Filter(77u).Single().Id == 10,
                    "Updated index cache"
                );
                updated = true;
            };
            conn.Reducers.MyAuth.Update(10, 77);
            Wait(() => updated, "namespaced update");
            var remote = conn.Db.MyAuth.User.RemoteQuery("WHERE id = 10");
            Wait(() => remote.IsCompleted, "namespaced RemoteQuery");
            Require(
                remote.GetAwaiter().GetResult().Single().Score == 77,
                "RemoteQuery result decoding"
            );
            var privateDenied = false;
            conn.SubscriptionBuilder()
                .OnApplied(_ => throw new Exception("A non-owner subscribed to private child data"))
                .OnError((_, _) => privateDenied = true)
                .Subscribe(new[] { "SELECT * FROM \"MyAuth\".secret" });
            Wait(() => privateDenied, "private child subscription rejection");

            var failed = false;
            conn.OnUnhandledReducerError += (_, error) =>
            {
                Require(error.Message.Contains("namespace rollback"), "Unexpected reducer failure");
                failed = true;
            };
            conn.Reducers.MyAuth.Fail(999);
            Wait(() => failed, "child error forwarded to root");
            var rolledBack = conn.Db.MyAuth.User.RemoteQuery("WHERE id = 999");
            Wait(() => rolledBack.IsCompleted, "rollback verification");
            Require(
                rolledBack.GetAwaiter().GetResult().Length == 0,
                "Failed reducer committed a row"
            );

            var deleted = false;
            conn.Db.MyAuth.User.OnDelete += (ctx, row) =>
            {
                if (row.Id != 10)
                    return;
                Require(
                    ctx.Db.MyAuth.User.Id.Find(10) == null
                        && ctx.Db.@class.User.Id.Find(10) != null,
                    "Deletion crossed namespace boundaries"
                );
                deleted = true;
            };
            conn.Reducers.MyAuth.Remove(10);
            Wait(() => deleted, "namespaced deletion");
            Unsubscribe(subscription);
            EmptyCache();
            var uncached = conn.Db.MyAuth.User.RemoteQuery("");
            Wait(() => uncached.IsCompleted, "unsubscribed RemoteQuery");
            Require(
                uncached.GetAwaiter().GetResult().Single().Id == 2,
                "Unsubscribed RemoteQuery result"
            );
            EmptyCache();

            var allApplied = false;
            var all = conn.SubscriptionBuilder()
                .OnApplied(_ => allApplied = true)
                .OnError((_, error) => throw error)
                .SubscribeToAllTables();
            Wait(() => allApplied, "subscribe-all initial rows");
            Require(
                conn.Db.User.Count == 1
                    && conn.Db.MyAuth.User.Count == 1
                    && conn.Db.@class.User.Count == 2
                    && conn.Db.ExtraRow.Count == 1
                    && conn.Db.Users.Count == 1
                    && conn.Db.MyAuth.Users.Count == 1
                    && conn.Db.MyAuth.AnonymousUsers.Count == 1
                    && conn.Db.MyAuth.QueryUsers.Count == 1
                    && conn.Db.AuthUsers.Count == 1
                    && conn.Db.QueryUsers.Count == 1
                    && conn.Db.QueryUsersRight.Count == 1
                    && conn.Db.QueryExtra.Count == 1,
                "Subscribe-all omitted child tables or views"
            );
            Require(notices == 1, "Event rows must not be replayed as persistent rows");
            Unsubscribe(all);
            EmptyCache();
            Console.WriteLine("Namespace integration passed");
        }
        finally
        {
            conn.Disconnect();
        }
    }
}
