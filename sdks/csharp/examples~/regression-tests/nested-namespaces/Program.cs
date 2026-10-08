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
        {
            throw new Exception(message);
        }
    }

    private static void Main()
    {
        var host =
            Environment.GetEnvironmentVariable("SPACETIMEDB_SERVER_URL") ?? "http://localhost:3000";
        if (host == "local")
        {
            host = "http://localhost:3000";
        }
        var connected = false;
        Exception? connectionError = null;
        var conn = DbConnection
            .Builder()
            .WithUri(host)
            .WithDatabaseName("nested-namespace-tests")
            .OnConnect((_, _, _) => connected = true)
            .OnConnectError(error => connectionError = error)
            .Build();

        void Wait(Func<bool> done, string phase)
        {
            var timer = Stopwatch.StartNew();
            while (!done())
            {
                if (connectionError != null)
                {
                    throw connectionError;
                }
                if (timer.Elapsed > TimeSpan.FromSeconds(30))
                {
                    throw new TimeoutException(phase);
                }
                conn.FrameTick();
                Thread.Sleep(5);
            }
        }

        void Unsubscribe(SubscriptionHandle subscription)
        {
            var done = false;
            subscription.UnsubscribeThen(_ => done = true);
            Wait(() => done, "unsubscribe");
        }

        void Procedure(Action<ProcedureCallback<int>> call, int expected)
        {
            var done = false;
            call(
                (_, result) =>
                {
                    Require(
                        result.IsSuccess && result.Value == expected,
                        "Wrong procedure instance/result"
                    );
                    done = true;
                }
            );
            Wait(() => done, "procedure result");
        }

        try
        {
            Wait(() => connected, "connect");
            var deep = conn.Db.@class.Branch.Leaf.User;
            var sibling = conn.Db.Branch.Leaf.User;
            var allQueries = QueryBuilder.AllTablesSqlQueries();
            Require(
                allQueries.Length == allQueries.Distinct().Count(),
                "Subscribe-all does not duplicate tables/views"
            );

            var applied = false;
            var subscription = conn.SubscriptionBuilder()
                .OnApplied(_ => applied = true)
                .OnError((_, error) => throw error)
                .AddQuery(q => q.From.@class.Branch.Leaf.User())
                .AddQuery(q => q.From.@class.Branch.Leaf.RefUser())
                .AddQuery(q => q.From.Branch.Leaf.User())
                .Subscribe();
            Wait(() => applied, "typed nested subscription");

            var inserts = 0;
            var deletes = 0;
            void CheckInsert(EventContext ctx, SpacetimeDB.Types.@class.Branch.Leaf.User row)
            {
                Require(
                    ctx.Db.@class.Branch.Leaf.RefUser.Id.Find(1) != null,
                    "Callback sees shared post-transaction cache"
                );
                inserts++;
            }
            deep.OnInsert += CheckInsert;
            deep.OnDelete += (_, _) => deletes++;
            var rootEvents = 0;
            var siblingEvents = 0;
            var deepEvents = 0;
            conn.Reducers.OnPing += _ => rootEvents++;
            conn.Reducers.Branch.Leaf.OnPing += _ => siblingEvents++;
            conn.Reducers.@class.Branch.Leaf.OnPing += ctx =>
            {
                Require(ctx.Event.Status is Status.Committed, "Deep reducer failed");
                deepEvents++;
            };
            conn.Reducers.Ping();
            conn.Reducers.Branch.Leaf.Ping();
            conn.Reducers.@class.Branch.Leaf.Ping();
            Wait(
                () => rootEvents == 1 && siblingEvents == 1 && deepEvents == 1 && inserts == 1,
                "reducer dispatch and inserts"
            );
            deep.OnInsert -= CheckInsert;
            Require(
                deep.Id.Find(1)?.Value == 108 && sibling.Id.Find(1)?.Value == 102,
                "Separate caches for the same Leaf DLL"
            );

            var errors = 0;
            conn.OnUnhandledReducerError += (_, _) => errors++;
            conn.Reducers.@class.Branch.Leaf.Pong();
            conn.Reducers.Branch.Leaf.Pong();
            Wait(() => deletes == 1 && deep.Count == 0 && sibling.Count == 0, "nested deletes");
            // A second Pong fails; its error must propagate through every parent container.
            conn.Reducers.@class.Branch.Leaf.Pong();
            Wait(() => errors == 1, "nested unhandled reducer error");
            Unsubscribe(subscription);

            // Branch expects its child's rows, which have not been created yet.
            var failed = false;
            conn.Procedures.@class.Branch.Instance(
                (_, result) =>
                {
                    Require(!result.IsSuccess && result.Error != null, "Nested procedure failure");
                    failed = true;
                }
            );
            Wait(() => failed, "procedure failure result");
            Procedure(conn.Procedures.Instance, 0);
            Procedure(conn.Procedures.Branch.Leaf.Next, 12);
            Procedure(conn.Procedures.Leaf.Next, 13);
            Procedure(conn.Procedures.Promoted.Next, 14);
            Procedure(conn.Procedures.SecondLeaf.Next, 15);
            Procedure(conn.Procedures.@class.Next, 16);
            Procedure(conn.Procedures.@class.Branch.Leaf.Next, 18);
            Procedure(conn.Procedures.@class.Branch.Instance, 7);

            var remote = deep.RemoteQuery("WHERE id = 1");
            Wait(() => remote.IsCompleted, "deep RemoteQuery");
            Require(
                remote.Result.Single().Value == 8 && deep.Count == 0,
                "One-off rows use the correct type without populating the cache"
            );

            applied = false;
            subscription = conn.SubscriptionBuilder()
                .OnApplied(_ => applied = true)
                .OnError((_, error) => throw error)
                .SubscribeToAllTables();
            Wait(() => applied, "subscribe all nested tables and views");
            Require(deep.Count == 2 && sibling.Count == 2, "Initial rows reach separate handles");
            Require(
                conn.Db.Leaf.User.Id.Find(1)?.Value == 3
                    && conn.Db.Promoted.User.Id.Find(1)?.Value == 4
                    && conn.Db.SecondLeaf.User.Id.Find(1)?.Value == 5
                    && conn.Db.@class.User.Id.Find(1)?.Value == 6,
                "Public promotion and repeated instances"
            );
            Require(
                conn.Db.@class.Branch.Leaf.Current.Iter().Single().Value == 8
                    && conn.Db.@class.Branch.Leaf.Anonymous.Iter().Single().Value == 18
                    && conn.Db.Deep.Iter().Single().Value == 8,
                "Nested and root view results"
            );
            Unsubscribe(subscription);
            Require(
                deep.Count == 0
                    && sibling.Count == 0
                    && conn.Db.@class.User.Count == 0
                    && conn.Db.Leaf.User.Count == 0
                    && conn.Db.Promoted.User.Count == 0
                    && conn.Db.SecondLeaf.User.Count == 0
                    && conn.Db.@class.Branch.Leaf.Current.Count == 0
                    && conn.Db.@class.Branch.Leaf.Anonymous.Count == 0,
                "Unsubscribe clears nested table and view caches"
            );

            applied = false;
            subscription = conn.SubscriptionBuilder()
                .OnApplied(_ => applied = true)
                .OnError((_, error) => throw error)
                .AddQuery(q =>
                    q.From.@class.Branch.Leaf.User()
                        .Where(c => c.Id.Eq(1))
                        .LeftSemijoin(
                            q.From.Branch.Leaf.User(),
                            (left, right) => left.Id.Eq(right.Id)
                        )
                )
                .Subscribe();
            Wait(() => applied, "same-leaf cross-namespace join");
            Require(
                deep.Count == 1 && deep.Id.Find(1)?.Value == 8 && sibling.Count == 0,
                "Join routes only the projected table"
            );
            Unsubscribe(subscription);
            Require(deep.Count == 0, "Join unsubscribe cleanup");
            Console.WriteLine("Nested namespace client passed");
        }
        finally
        {
            conn.Disconnect();
        }
    }
}
