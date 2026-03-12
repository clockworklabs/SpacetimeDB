/// View-PK regression tests run with a live server.
/// To run these, start a local SpacetimeDB via `spacetime start`,
/// publish `modules/sdk-test-view-pk-cs`, and then run this client.
using System.Threading;
using RegressionTests.Shared;
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

RegressionTestHarness.RegisterUnhandledExceptionExitHandler();
RegressionTestHarness.RunNamedTests(args, tests);
Log.Info("Success");
Environment.Exit(0);

void RunViewPkTest(string testName, Action<DbConnection, Action, Action<Exception>> start)
{
    RegressionTestHarness.RunLiveConnectionTest(HOST, DBNAME, testName, TIMEOUT_SECONDS, start);
}

void RunOrFail(Action work, Action<Exception> fail)
{
    try
    {
        work();
    }
    catch (Exception ex)
    {
        fail(ex);
    }
}

void AssertCommittedOrFail(string reducerName, ReducerEventContext ctx, Action<Exception> fail)
{
    RunOrFail(() => RegressionTestHarness.AssertReducerCommitted(reducerName, ctx), fail);
}

void ExpectSinglePlayerUpdate(
    string testName,
    ref bool sawUpdate,
    ulong expectedId,
    string expectedOldName,
    string expectedNewName,
    ulong oldId,
    string oldName,
    ulong newId,
    string newName
)
{
    RegressionTestHarness.Require(
        !sawUpdate,
        $"Expected exactly one OnUpdate callback for {testName}."
    );
    RegressionTestHarness.Require(oldId == expectedId, $"Expected oldRow.Id={expectedId}, got {oldId}.");
    RegressionTestHarness.Require(
        oldName == expectedOldName,
        $"Expected oldRow.Name={expectedOldName}, got {oldName}."
    );
    RegressionTestHarness.Require(newId == expectedId, $"Expected newRow.Id={expectedId}, got {newId}.");
    RegressionTestHarness.Require(
        newName == expectedNewName,
        $"Expected newRow.Name={expectedNewName}, got {newName}."
    );
    sawUpdate = true;
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
    const string testName = "view-pk-on-update";
    var playerId = NextId();
    const string before = "before";
    const string after = "after";

    RunViewPkTest(testName, (conn, pass, fail) =>
    {
        bool sawUpdate = false;

        conn.Reducers.OnInsertViewPkPlayer += (ctx, _, _) =>
            AssertCommittedOrFail("insert_view_pk_player", ctx, fail);
        conn.Reducers.OnUpdateViewPkPlayer += (ctx, _, _) =>
            AssertCommittedOrFail("update_view_pk_player", ctx, fail);

        conn
            .SubscriptionBuilder()
            .OnApplied(ctx =>
                RunOrFail(
                    () =>
                    {
                        ctx.Db.AllViewPkPlayers.OnUpdate += (_, oldRow, newRow) =>
                            RunOrFail(
                                () =>
                                {
                                    ExpectSinglePlayerUpdate(
                                        testName,
                                        ref sawUpdate,
                                        playerId,
                                        before,
                                        after,
                                        oldRow.Id,
                                        oldRow.Name,
                                        newRow.Id,
                                        newRow.Name
                                    );
                                    pass();
                                },
                                fail
                            );

                        ctx.Reducers.InsertViewPkPlayer(playerId, before);
                        ctx.Reducers.UpdateViewPkPlayer(playerId, after);
                    },
                    fail
                )
            )
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
    const string testName = "view-pk-join-query-builder";
    var playerId = NextId();
    var membershipId = NextId();
    const string before = "before";
    const string after = "after";

    RunViewPkTest(testName, (conn, pass, fail) =>
    {
        bool sawUpdate = false;

        conn.Reducers.OnInsertViewPkPlayer += (ctx, _, _) =>
            AssertCommittedOrFail("insert_view_pk_player", ctx, fail);
        conn.Reducers.OnInsertViewPkMembership += (ctx, _, _) =>
            AssertCommittedOrFail("insert_view_pk_membership", ctx, fail);
        conn.Reducers.OnUpdateViewPkPlayer += (ctx, _, _) =>
            AssertCommittedOrFail("update_view_pk_player", ctx, fail);

        conn
            .SubscriptionBuilder()
            .AddQuery(q =>
                q.From.ViewPkMembership().RightSemijoin(
                    q.From.AllViewPkPlayers(),
                    (membership, player) => membership.PlayerId.Eq(player.Id)
                )
            )
            .OnApplied(ctx =>
                RunOrFail(
                    () =>
                    {
                        ctx.Db.AllViewPkPlayers.OnUpdate += (_, oldRow, newRow) =>
                            RunOrFail(
                                () =>
                                {
                                    ExpectSinglePlayerUpdate(
                                        testName,
                                        ref sawUpdate,
                                        playerId,
                                        before,
                                        after,
                                        oldRow.Id,
                                        oldRow.Name,
                                        newRow.Id,
                                        newRow.Name
                                    );
                                    pass();
                                },
                                fail
                            );

                        ctx.Reducers.InsertViewPkPlayer(playerId, before);
                        ctx.Reducers.InsertViewPkMembership(membershipId, playerId);
                        ctx.Reducers.UpdateViewPkPlayer(playerId, after);
                    },
                    fail
                )
            )
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
    const string testName = "view-pk-semijoin-two-sender-views-query-builder";
    var playerId = NextId();
    var membershipAId = NextId();
    var membershipBId = NextId();
    const string before = "before";
    const string after = "after";

    RunViewPkTest(testName, (conn, pass, fail) =>
    {
        bool sawUpdate = false;

        conn.Reducers.OnInsertViewPkPlayer += (ctx, _, _) =>
            AssertCommittedOrFail("insert_view_pk_player", ctx, fail);
        conn.Reducers.OnInsertViewPkMembership += (ctx, _, _) =>
            AssertCommittedOrFail("insert_view_pk_membership", ctx, fail);
        conn.Reducers.OnInsertViewPkMembershipSecondary += (ctx, _, _) =>
            AssertCommittedOrFail("insert_view_pk_membership_secondary", ctx, fail);
        conn.Reducers.OnUpdateViewPkPlayer += (ctx, _, _) =>
            AssertCommittedOrFail("update_view_pk_player", ctx, fail);

        conn
            .SubscriptionBuilder()
            .AddQuery(q =>
                q.From.SenderViewPkPlayersA().RightSemijoin(
                    q.From.SenderViewPkPlayersB(),
                    (lhsView, rhsView) => lhsView.Id.Eq(rhsView.Id)
                )
            )
            .OnApplied(ctx =>
                RunOrFail(
                    () =>
                    {
                        ctx.Db.SenderViewPkPlayersB.OnUpdate += (_, oldRow, newRow) =>
                            RunOrFail(
                                () =>
                                {
                                    ExpectSinglePlayerUpdate(
                                        testName,
                                        ref sawUpdate,
                                        playerId,
                                        before,
                                        after,
                                        oldRow.Id,
                                        oldRow.Name,
                                        newRow.Id,
                                        newRow.Name
                                    );
                                    pass();
                                },
                                fail
                            );

                        ctx.Reducers.InsertViewPkPlayer(playerId, before);
                        ctx.Reducers.InsertViewPkMembership(membershipAId, playerId);
                        ctx.Reducers.InsertViewPkMembershipSecondary(membershipBId, playerId);
                        ctx.Reducers.UpdateViewPkPlayer(playerId, after);
                    },
                    fail
                )
            )
            .OnError((_, err) => fail(err))
            .Subscribe();
    });
}
