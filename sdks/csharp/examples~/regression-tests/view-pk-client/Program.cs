/// View-PK regression tests run with a live server.
/// To run these, start a local SpacetimeDB via `spacetime start`,
/// publish `modules/sdk-test-view-pk-cs`, and then run this client.
using System.Threading;
using SpacetimeDB;
using SpacetimeDB.Types;

const string HOST = "http://localhost:3000";
const string DBNAME = "view-pk-tests";
const int TIMEOUT_SECONDS = 20;

long idCounter = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() * 10;
ulong NextId() => (ulong)Interlocked.Increment(ref idCounter);

var tests = new Dictionary<string, Action>(StringComparer.Ordinal)
{
    ["view-pk-on-update"] = ExecViewPkOnUpdate,
    ["view-pk-join-query-builder"] = ExecViewPkJoinQueryBuilder,
    ["view-pk-semijoin-two-sender-views-query-builder"] =
        ExecViewPkSemijoinTwoSenderViewsQueryBuilder,
};

if (args.Length > 1)
{
    throw new ArgumentException("Pass zero args (run all) or a single test name.");
}

System.AppDomain.CurrentDomain.UnhandledException += (sender, evt) =>
{
    Log.Exception($"Unhandled exception: {sender} {evt}");
    Environment.Exit(1);
};

if (args.Length == 1)
{
    var testName = args[0];
    if (!tests.TryGetValue(testName, out var test))
    {
        throw new ArgumentException($"Unknown test: {testName}");
    }

    Log.Info($"Running {testName}");
    test();
}
else
{
    foreach (var (testName, test) in tests)
    {
        Log.Info($"Running {testName}");
        test();
    }
}

Log.Info("Success");
Environment.Exit(0);

void Expect(bool condition, string message)
{
    if (!condition)
    {
        throw new Exception(message);
    }
}

void AssertReducerCommitted(string reducerName, ReducerEventContext ctx)
{
    switch (ctx.Event.Status)
    {
        case Status.Committed:
            return;
        case Status.Failed(var reason):
            throw new Exception($"`{reducerName}` reducer returned error: {reason}");
        case Status.OutOfEnergy(var _):
            throw new Exception($"`{reducerName}` reducer ran out of energy");
        default:
            throw new Exception($"`{reducerName}` reducer returned unexpected status: {ctx.Event.Status}");
    }
}

void RunViewPkTest(
    string testName,
    Action<DbConnection, Action, Action<Exception>> start
)
{
    bool complete = false;
    bool disconnectExpected = false;
    Exception? failure = null;

    void Pass()
    {
        complete = true;
    }

    void Fail(Exception error)
    {
        failure ??= error;
    }

    var conn = DbConnection
        .Builder()
        .WithUri(HOST)
        .WithDatabaseName(DBNAME)
        .OnConnect((connected, _, _) =>
        {
            try
            {
                start(connected, Pass, Fail);
            }
            catch (Exception ex)
            {
                Fail(ex);
            }
        })
        .OnConnectError(err =>
        {
            Fail(err);
        })
        .OnDisconnect((_, err) =>
        {
            if (disconnectExpected)
            {
                return;
            }

            if (err != null)
            {
                Fail(err);
                return;
            }

            if (!complete)
            {
                Fail(new Exception($"Unexpected disconnect in {testName}"));
            }
        })
        .Build();

    var deadline = DateTime.UtcNow.AddSeconds(TIMEOUT_SECONDS);
    while (!complete && failure == null)
    {
        conn.FrameTick();
        Thread.Sleep(10);

        if (DateTime.UtcNow > deadline)
        {
            throw new TimeoutException($"Timeout waiting for {testName}");
        }
    }

    disconnectExpected = true;
    if (conn.IsActive)
    {
        conn.Disconnect();
    }

    if (failure != null)
    {
        throw new Exception($"{testName} failed", failure);
    }
}

/// Subscribe to a query builder view whose underlying table has a primary key.
/// Ensures the C# SDK emits an `OnUpdate` callback and that the client receives the correct old and new rows.
///
/// Test:
/// 1. Subscribe to: SELECT * FROM all_view_pk_players
/// 2. Insert row:  (id=1, name="before")
/// 3. Update row:  (id=1, name="after")
///
/// Expect:
/// - `OnUpdate` is called for PK=1
/// - `oldRow` should be the "before" value
/// - `newRow` should be the "after" value
void ExecViewPkOnUpdate()
{
    var playerId = NextId();
    const string before = "before";
    const string after = "after";

    RunViewPkTest("view-pk-on-update", (conn, pass, fail) =>
    {
        bool sawUpdate = false;

        conn.Reducers.OnInsertViewPkPlayer += (ctx, _, _) =>
        {
            try
            {
                AssertReducerCommitted("insert_view_pk_player", ctx);
            }
            catch (Exception ex)
            {
                fail(ex);
            }
        };

        conn.Reducers.OnUpdateViewPkPlayer += (ctx, _, _) =>
        {
            try
            {
                AssertReducerCommitted("update_view_pk_player", ctx);
            }
            catch (Exception ex)
            {
                fail(ex);
            }
        };

        conn
            .SubscriptionBuilder()
            .OnApplied(ctx =>
            {
                try
                {
                    ctx.Db.AllViewPkPlayers.OnUpdate += (_, oldRow, newRow) =>
                    {
                        try
                        {
                            Expect(!sawUpdate, "Expected exactly one OnUpdate callback for view-pk-on-update.");
                            Expect(oldRow.Id == playerId, $"Expected oldRow.Id={playerId}, got {oldRow.Id}.");
                            Expect(oldRow.Name == before, $"Expected oldRow.Name={before}, got {oldRow.Name}.");
                            Expect(newRow.Id == playerId, $"Expected newRow.Id={playerId}, got {newRow.Id}.");
                            Expect(newRow.Name == after, $"Expected newRow.Name={after}, got {newRow.Name}.");
                            sawUpdate = true;
                            pass();
                        }
                        catch (Exception ex)
                        {
                            fail(ex);
                        }
                    };

                    ctx.Reducers.InsertViewPkPlayer(playerId, before);
                    ctx.Reducers.UpdateViewPkPlayer(playerId, after);
                }
                catch (Exception ex)
                {
                    fail(ex);
                }
            })
            .OnError((_, err) => fail(err))
            .Subscribe(["SELECT * FROM all_view_pk_players"]);
    });
}

/// Subscribe to a right semijoin whose rhs is a view with primary key.
///
/// Ensures:
/// 1. A semijoin subscription involving a view is valid
/// 2. The C# SDK emits an `OnUpdate` callback and that the client receives the correct old and new rows
///
/// Query:
///   SELECT player.*
///   FROM view_pk_membership membership
///   JOIN all_view_pk_players player ON membership.player_id = player.id
///
/// Test:
/// 1. Insert player row (id=1, "before").
/// 2. Insert membership row referencing player_id=1, allowing the semijoin match.
/// 3. Update player row to (id=1, "after").
///
/// Expect:
/// - `OnUpdate` is called for player PK=1
/// - `oldRow` should be the "before" value
/// - `newRow` should be the "after" value
void ExecViewPkJoinQueryBuilder()
{
    var playerId = NextId();
    var membershipId = NextId();
    const string before = "before";
    const string after = "after";

    RunViewPkTest("view-pk-join-query-builder", (conn, pass, fail) =>
    {
        bool sawUpdate = false;

        conn.Reducers.OnInsertViewPkPlayer += (ctx, _, _) =>
        {
            try
            {
                AssertReducerCommitted("insert_view_pk_player", ctx);
            }
            catch (Exception ex)
            {
                fail(ex);
            }
        };

        conn.Reducers.OnInsertViewPkMembership += (ctx, _, _) =>
        {
            try
            {
                AssertReducerCommitted("insert_view_pk_membership", ctx);
            }
            catch (Exception ex)
            {
                fail(ex);
            }
        };

        conn.Reducers.OnUpdateViewPkPlayer += (ctx, _, _) =>
        {
            try
            {
                AssertReducerCommitted("update_view_pk_player", ctx);
            }
            catch (Exception ex)
            {
                fail(ex);
            }
        };

        conn
            .SubscriptionBuilder()
            .AddQuery(q =>
                q.From.ViewPkMembership().RightSemijoin(
                    q.From.AllViewPkPlayers(),
                    (membership, player) => membership.PlayerId.Eq(player.Id)
                )
            )
            .OnApplied(ctx =>
            {
                try
                {
                    ctx.Db.AllViewPkPlayers.OnUpdate += (_, oldRow, newRow) =>
                    {
                        try
                        {
                            Expect(!sawUpdate, "Expected exactly one OnUpdate callback for view-pk-join-query-builder.");
                            Expect(oldRow.Id == playerId, $"Expected oldRow.Id={playerId}, got {oldRow.Id}.");
                            Expect(oldRow.Name == before, $"Expected oldRow.Name={before}, got {oldRow.Name}.");
                            Expect(newRow.Id == playerId, $"Expected newRow.Id={playerId}, got {newRow.Id}.");
                            Expect(newRow.Name == after, $"Expected newRow.Name={after}, got {newRow.Name}.");
                            sawUpdate = true;
                            pass();
                        }
                        catch (Exception ex)
                        {
                            fail(ex);
                        }
                    };

                    ctx.Reducers.InsertViewPkPlayer(playerId, before);
                    ctx.Reducers.InsertViewPkMembership(membershipId, playerId);
                    ctx.Reducers.UpdateViewPkPlayer(playerId, after);
                }
                catch (Exception ex)
                {
                    fail(ex);
                }
            })
            .OnError((_, err) => fail(err))
            .Subscribe();
    });
}

/// Subscribe to a semijoin between two views with primary keys.
///
/// Ensures:
/// 1. A semijoin subscription involving a view is valid
/// 2. The C# SDK emits an `OnUpdate` callback and that the client receives the correct old and new rows
///
/// Query:
///   SELECT b.*
///   FROM sender_view_pk_players_a a
///   JOIN sender_view_pk_players_b b ON a.id = b.id
///
/// Test:
/// 1. Insert player row (id=1, "before").
/// 2. Insert membership for sender view A.
/// 3. Insert membership for sender view B.
/// 4. Update player row to (id=1, "after").
///
/// Expect:
/// - `OnUpdate` is called for player PK=1
/// - `oldRow` should be the "before" value
/// - `newRow` should be the "after" value
void ExecViewPkSemijoinTwoSenderViewsQueryBuilder()
{
    var playerId = NextId();
    var membershipAId = NextId();
    var membershipBId = NextId();
    const string before = "before";
    const string after = "after";

    RunViewPkTest(
        "view-pk-semijoin-two-sender-views-query-builder",
        (conn, pass, fail) =>
        {
            bool sawUpdate = false;

            conn.Reducers.OnInsertViewPkPlayer += (ctx, _, _) =>
            {
                try
                {
                    AssertReducerCommitted("insert_view_pk_player", ctx);
                }
                catch (Exception ex)
                {
                    fail(ex);
                }
            };

            conn.Reducers.OnInsertViewPkMembership += (ctx, _, _) =>
            {
                try
                {
                    AssertReducerCommitted("insert_view_pk_membership", ctx);
                }
                catch (Exception ex)
                {
                    fail(ex);
                }
            };

            conn.Reducers.OnInsertViewPkMembershipSecondary += (ctx, _, _) =>
            {
                try
                {
                    AssertReducerCommitted("insert_view_pk_membership_secondary", ctx);
                }
                catch (Exception ex)
                {
                    fail(ex);
                }
            };

            conn.Reducers.OnUpdateViewPkPlayer += (ctx, _, _) =>
            {
                try
                {
                    AssertReducerCommitted("update_view_pk_player", ctx);
                }
                catch (Exception ex)
                {
                    fail(ex);
                }
            };

            conn
                .SubscriptionBuilder()
                .AddQuery(q =>
                    q.From.SenderViewPkPlayersA().RightSemijoin(
                        q.From.SenderViewPkPlayersB(),
                        (lhsView, rhsView) => lhsView.Id.Eq(rhsView.Id)
                    )
                )
                .OnApplied(ctx =>
                {
                    try
                    {
                        ctx.Db.SenderViewPkPlayersB.OnUpdate += (_, oldRow, newRow) =>
                        {
                            try
                            {
                                Expect(!sawUpdate, "Expected exactly one OnUpdate callback for view-pk-semijoin-two-sender-views-query-builder.");
                                Expect(oldRow.Id == playerId, $"Expected oldRow.Id={playerId}, got {oldRow.Id}.");
                                Expect(oldRow.Name == before, $"Expected oldRow.Name={before}, got {oldRow.Name}.");
                                Expect(newRow.Id == playerId, $"Expected newRow.Id={playerId}, got {newRow.Id}.");
                                Expect(newRow.Name == after, $"Expected newRow.Name={after}, got {newRow.Name}.");
                                sawUpdate = true;
                                pass();
                            }
                            catch (Exception ex)
                            {
                                fail(ex);
                            }
                        };

                        ctx.Reducers.InsertViewPkPlayer(playerId, before);
                        ctx.Reducers.InsertViewPkMembership(membershipAId, playerId);
                        ctx.Reducers.InsertViewPkMembershipSecondary(membershipBId, playerId);
                        ctx.Reducers.UpdateViewPkPlayer(playerId, after);
                    }
                    catch (Exception ex)
                    {
                        fail(ex);
                    }
                })
                .OnError((_, err) => fail(err))
                .Subscribe();
        }
    );
}
