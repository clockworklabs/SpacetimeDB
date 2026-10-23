using System;
using System.Diagnostics;
using System.Linq;
using System.Threading;
using SpacetimeDB;
using SpacetimeDB.Types;

internal static class RootRlsTests
{
    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new Exception(message);
        }
    }

    public static void Run(string host)
    {
        Identity? firstIdentity = null;
        Identity? secondIdentity = null;
        Exception? error = null;
        var first = DbConnection
            .Builder()
            .WithUri(host)
            .WithDatabaseName("namespace-tests")
            .OnConnect((_, identity, _) => firstIdentity = identity)
            .OnConnectError(e => error = e)
            .Build();
        var second = DbConnection
            .Builder()
            .WithUri(host)
            .WithDatabaseName("namespace-tests")
            .OnConnect((_, identity, _) => secondIdentity = identity)
            .OnConnectError(e => error = e)
            .Build();

        void Wait(Func<bool> done, string phase)
        {
            var timer = Stopwatch.StartNew();
            while (!done())
            {
                if (error != null)
                {
                    throw error;
                }

                if (timer.Elapsed > TimeSpan.FromSeconds(30))
                {
                    throw new TimeoutException("Root RLS: " + phase);
                }

                first.FrameTick();
                second.FrameTick();
                Thread.Sleep(5);
            }
            if (error != null)
            {
                throw error;
            }
        }

        try
        {
            Wait(() => firstIdentity != null && secondIdentity != null, "connect");
            var firstOwner = firstIdentity ?? throw new Exception("Missing first identity");
            var secondOwner = secondIdentity ?? throw new Exception("Missing second identity");
            Require(!firstOwner.Equals(secondOwner), "RLS clients must have distinct identities");
            var writes = 0;
            first.Reducers.OnWriteProtectedRow += (ctx, _, _, _) =>
            {
                Require(ctx.Event.Status is Status.Committed, "RLS fixture write failed");
                writes++;
            };
            void Write(uint id, Identity owner, uint value)
            {
                var expected = writes + 1;
                first.Reducers.WriteProtectedRow(id, owner, value);
                Wait(() => writes == expected, "write");
            }

            Write(1, firstOwner, 10);
            Write(2, secondOwner, 20);

            var inserts = new int[2];
            var updates = new int[2];
            var deletes = new int[2];
            var clients = new[] { first, second };
            var identities = new[] { firstOwner, secondOwner };
            for (var i = 0; i < clients.Length; i++)
            {
                var index = i;
                clients[i].Db.MyAuth.ProtectedRow.OnInsert += (_, row) =>
                {
                    Require(row.Owner.Equals(identities[index]), "RLS leaked an insert");
                    inserts[index]++;
                };
                clients[i].Db.MyAuth.ProtectedRow.OnUpdate += (_, before, after) =>
                {
                    Require(
                        before.Owner.Equals(identities[index])
                            && after.Owner.Equals(identities[index]),
                        "RLS leaked an update"
                    );
                    updates[index]++;
                };
                clients[i].Db.MyAuth.ProtectedRow.OnDelete += (_, row) =>
                {
                    Require(row.Owner.Equals(identities[index]), "RLS leaked a deletion");
                    deletes[index]++;
                };
            }

            var applied = 0;
            var firstSub = first
                .SubscriptionBuilder()
                .OnApplied(_ => applied++)
                .OnError((_, e) => error = e)
                .AddQuery(q => q.From.MyAuth.ProtectedRow())
                .Subscribe();
            var secondSub = second
                .SubscriptionBuilder()
                .OnApplied(_ => applied++)
                .OnError((_, e) => error = e)
                .Subscribe(new[] { "SELECT * FROM \"auth_data\".protected_row" });
            Wait(() => applied == 2, "initial subscriptions");
            Require(first.Db.MyAuth.ProtectedRow.Iter().Single().Id == 1, "First initial RLS rows");
            Require(
                second.Db.MyAuth.ProtectedRow.Iter().Single().Id == 2,
                "Second initial RLS rows"
            );

            Write(1, firstOwner, 11);
            Write(2, secondOwner, 21);
            Wait(() => updates[0] == 1 && updates[1] == 1, "visible updates");
            Require(
                first.Db.MyAuth.ProtectedRow.Iter().Single().Value == 11,
                "First updated value"
            );
            Require(
                second.Db.MyAuth.ProtectedRow.Iter().Single().Value == 21,
                "Second updated value"
            );

            // Changing ownership must remove the old owner's row and insert it for the new owner.
            Write(1, secondOwner, 12);
            Wait(() => deletes[0] == 1 && inserts[1] == 2, "ownership transfer");
            Require(first.Db.MyAuth.ProtectedRow.Count == 0, "Old owner retained the row");
            Require(second.Db.MyAuth.ProtectedRow.Count == 2, "New owner did not receive the row");
            Write(3, firstOwner, 30);
            Wait(() => inserts[0] == 2, "live insert");

            var firstQuery = first.Db.MyAuth.ProtectedRow.RemoteQuery("");
            var secondQuery = second.Db.MyAuth.ProtectedRow.RemoteQuery("");
            Wait(() => firstQuery.IsCompleted && secondQuery.IsCompleted, "filtered RemoteQuery");
            Require(firstQuery.GetAwaiter().GetResult().Single().Id == 3, "First RemoteQuery RLS");
            Require(
                secondQuery
                    .GetAwaiter()
                    .GetResult()
                    .Select(row => row.Id)
                    .OrderBy(id => id)
                    .SequenceEqual(new uint[] { 1, 2 }),
                "Second RemoteQuery RLS"
            );
            Require(
                inserts[0] == 2
                    && inserts[1] == 2
                    && updates[0] == 1
                    && updates[1] == 1
                    && deletes[0] == 1
                    && deletes[1] == 0,
                "Hidden changes must not produce callbacks"
            );

            var unsubscribed = 0;
            firstSub.UnsubscribeThen(_ => unsubscribed++);
            secondSub.UnsubscribeThen(_ => unsubscribed++);
            Wait(() => unsubscribed == 2, "unsubscribe");
            Require(
                first.Db.MyAuth.ProtectedRow.Count == 0 && second.Db.MyAuth.ProtectedRow.Count == 0,
                "RLS unsubscribe must clear both caches"
            );
            Console.WriteLine("Root-defined RLS on a namespace table passed");
        }
        finally
        {
            first.Disconnect();
            second.Disconnect();
        }
    }
}
