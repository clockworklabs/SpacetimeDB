/// View primary key tests run with a live server.
/// To run these, run a local SpacetimeDB via `spacetime start`,
/// then in a separate terminal run `tools~/run-regression-tests.sh`.

using System.Diagnostics;
using SpacetimeDB;
using SpacetimeDB.Types;

const string HOST = "http://localhost:3000";
const string DBNAME = "view-pk-tests";
const int TIMEOUT_SECONDS = 20;

DbConnection Connect(Action<DbConnection, Identity, string> onConnect)
{
    return DbConnection.Builder()
        .WithUri(HOST)
        .WithDatabaseName(DBNAME)
        .OnConnect(onConnect)
        .OnConnectError((err) =>
        {
            throw err;
        })
        .OnDisconnect((_, err) =>
        {
            if (err != null)
            {
                throw err;
            }

            throw new Exception("Unexpected disconnect");
        })
        .Build();
}

void WaitFor(DbConnection db, Func<bool> isDone, string testName)
{
    var start = DateTime.Now;
    while (!isDone())
    {
        db.FrameTick();
        Thread.Sleep(100);
        if ((DateTime.Now - start).TotalSeconds > TIMEOUT_SECONDS)
        {
            throw new Exception($"{testName} timed out after {TIMEOUT_SECONDS} seconds");
        }
    }
}

void RunOnUpdateTest()
{
    Log.Info("Running view-pk on-update test");

    var applied = false;
    var onUpdateCalled = false;

    var db = Connect((conn, _, _) =>
    {
        conn.SubscriptionBuilder()
            .OnApplied((ctx) =>
            {
                if (applied)
                {
                    return;
                }
                applied = true;

                ctx.Db.AllViewPkPlayers.OnUpdate += (_, oldRow, newRow) =>
                {
                    Debug.Assert(oldRow.Id == 1UL, $"Expected old id=1, got {oldRow.Id}");
                    Debug.Assert(oldRow.Name == "before", $"Expected old name=before, got {oldRow.Name}");
                    Debug.Assert(newRow.Id == 1UL, $"Expected new id=1, got {newRow.Id}");
                    Debug.Assert(newRow.Name == "after", $"Expected new name=after, got {newRow.Name}");
                    onUpdateCalled = true;
                };

                ctx.Procedures.InsertViewPkPlayer(1UL, "before", (_, result) =>
                {
                    if (!result.IsSuccess)
                    {
                        throw result.Error!;
                    }
                });

                ctx.Procedures.UpdateViewPkPlayer(1UL, "after", (_, result) =>
                {
                    if (!result.IsSuccess)
                    {
                        throw result.Error!;
                    }
                });
            })
            .OnError((_, err) =>
            {
                throw err;
            })
            .AddQuery(qb => qb.From.AllViewPkPlayers())
            .Subscribe();
    });

    WaitFor(db, () => applied && onUpdateCalled, "view-pk on-update test");
    db.Disconnect();
}

void RunJoinQueryBuilderTest()
{
    Log.Info("Running view-pk join query-builder test");

    var applied = false;
    var onInsertCalled = false;

    var db = Connect((conn, _, _) =>
    {
        conn.SubscriptionBuilder()
            .OnApplied((ctx) =>
            {
                if (applied)
                {
                    return;
                }
                applied = true;

                ctx.Db.AllViewPkPlayers.OnInsert += (_, row) =>
                {
                    Debug.Assert(row.Id == 2UL, $"Expected joined row id=2, got {row.Id}");
                    Debug.Assert(row.Name == "joined", $"Expected joined row name=joined, got {row.Name}");
                    onInsertCalled = true;
                };

                ctx.Procedures.InsertViewPkPlayer(2UL, "joined", (_, result) =>
                {
                    if (!result.IsSuccess)
                    {
                        throw result.Error!;
                    }
                });

                ctx.Procedures.InsertViewPkMembership(1UL, 2UL, (_, result) =>
                {
                    if (!result.IsSuccess)
                    {
                        throw result.Error!;
                    }
                });
            })
            .OnError((_, err) =>
            {
                throw err;
            })
            .AddQuery(qb =>
                qb.From.ViewPkMembership().RightSemijoin(qb.From.AllViewPkPlayers(), (m, p) => m.PlayerId.Eq(p.Id))
            )
            .Subscribe();
    });

    WaitFor(db, () => applied && onInsertCalled, "view-pk join query-builder test");
    db.Disconnect();
}

System.AppDomain.CurrentDomain.UnhandledException += (_, args) =>
{
    Log.Exception($"Unhandled exception: {args}");
    Environment.Exit(1);
};

RunOnUpdateTest();
RunJoinQueryBuilderTest();

Log.Info("Success");
Environment.Exit(0);
