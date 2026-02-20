///  tests run with a live server.
/// To run these, run a local SpacetimeDB via `spacetime start`,
/// then in a separate terminal run `tools~/run-regression-tests.sh PATH_TO_SPACETIMEDB_REPO_CHECKOUT`.
/// This is done on CI in .github/workflows/test.yml.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Threading;
using SpacetimeDB;
using SpacetimeDB.Types;

const string HOST = "http://localhost:3000";
const string DBNAME = "btree-repro";
const string THROW_ERROR_MESSAGE = "this is an error";
const uint UPDATED_WHERE_TEST_VALUE = 42;
const string UPDATED_WHERE_TEST_NAME = "this_name_was_updated";
const string EXPECTED_TEST_EVENT_NAME = "hello";
const ulong EXPECTED_TEST_EVENT_VALUE = 42;

DbConnection ConnectToDB()
{
    DbConnection? conn = null;
    conn = DbConnection
        .Builder()
        .WithUri(HOST)
        .WithDatabaseName(DBNAME)
        .OnConnect(OnConnected)
        .OnConnectError(
            (err) =>
            {
                throw err;
            }
        )
        .OnDisconnect(
            (conn, err) =>
            {
                if (err != null)
                {
                    throw err;
                }
                else
                {
                    throw new Exception("Unexpected disconnect");
                }
            }
        )
        .Build();
    return conn;
}

uint waiting = 0;
var applied = false;
SubscriptionHandle? handle = null;
uint testEventInsertCount = 0;

void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    Log.Debug($"Connected to {DBNAME} on {HOST}");
    handle = conn.SubscriptionBuilder()
        .OnApplied(OnSubscriptionApplied)
        .OnError(
            (ctx, err) =>
            {
                throw err;
            }
        )
        .AddQuery(qb => qb.From.ExampleData().Build())
        .AddQuery(qb => qb.From.MyPlayer().Build())
        .AddQuery(qb => qb.From.MyAccount().Build())
        .AddQuery(qb => qb.From.MyAccountMissing().Build())
        .AddQuery(qb => qb.From.PlayersAtLevelOne().Build())
        .AddQuery(qb => qb.From.MyTable().Build())
        .AddQuery(qb => qb.From.NullStringNonnullable().Build())
        .AddQuery(qb => qb.From.NullStringNullable().Build())
        .AddQuery(qb => qb.From.MyLog().Build())
        .AddQuery(qb => qb.From.TestEvent().Build())
        .AddQuery(qb => qb.From.Admins().Build())
        .AddQuery(qb => qb.From.NullableVecView().Build())
        .AddQuery(qb => qb.From.WhereTest().Where(c => c.Value.Gt(10)).Build())
        .AddQuery(qb =>
            qb.From.Player()
                .LeftSemijoin(qb.From.PlayerLevel(), (p, pl) => p.Id.Eq(pl.PlayerId))
                .Build()
        )
        .AddQuery(qb =>
            qb.From.Player()
                .Where(c => c.Name.Eq("NewPlayer"))
                .RightSemijoin(qb.From.PlayerLevel(), (p, pl) => p.Id.Eq(pl.PlayerId))
                .Where(c => c.Level.Eq(1UL))
                .Build()
        )
        .AddQuery(qb => qb.From.UsersNamedAlice().Build())
        .AddQuery(qb => qb.From.UsersAge1865().Build())
        .AddQuery(qb => qb.From.UsersAge18Plus().Build())
        .AddQuery(qb => qb.From.UsersAgeUnder18().Build())
        .AddQuery(qb => qb.From.ScoresPlayer123().Build())
        .AddQuery(qb => qb.From.ScoresPlayer123Range().Build())
        .AddQuery(qb => qb.From.ScoresPlayer123Level5().Build())
        .AddQuery(qb =>
            qb.From.User()
                .Where(c => c.Age.Gte((byte)18).And(c.Age.Lt((byte)65)))
                .Where(c => c.IsAdmin.Eq(true).Or(c.Name.Eq("Charlie")))
                .Build()
        )
        .AddQuery(qb => qb.From.Score().Build())
        .AddQuery(qb => qb.From.WhereTestView().Build())
        .AddQuery(qb => qb.From.FindWhereTest().Build())
        .AddQuery(qb => qb.From.WhereTestQuery().Build())
        .Subscribe();

    // If testing against Rust, the indexed parameter will need to be changed to: ulong indexed
    conn.Reducers.OnAdd += (ReducerEventContext ctx, uint id, uint indexed) =>
    {
        Log.Info("Got Add callback");
        waiting--;
        ValidateBTreeIndexes(ctx);
    };

    conn.Reducers.OnDelete += (ReducerEventContext ctx, uint id) =>
    {
        Log.Info("Got Delete callback");
        waiting--;
        ValidateBTreeIndexes(ctx);
    };

    conn.Reducers.OnInsertResult += (ReducerEventContext ctx, Result<MyTable, string> msg) =>
    {
        Log.Info($"Got InsertResult callback: {msg}");
        waiting--;
    };

    conn.OnUnhandledReducerError += (ReducerEventContext ctx, Exception exception) =>
    {
        Log.Info($"Got OnUnhandledReducerError: {exception}");
        waiting--;
        ValidateReducerErrorDoesNotContainStackTrace(exception);
        ValidateBTreeIndexes(ctx);
        ValidateNullableVecView(ctx);
    };

    conn.Reducers.OnSetNullableVec += (
        ReducerEventContext ctx,
        uint id,
        bool hasPos,
        int x,
        int y
    ) =>
    {
        Log.Info("Got SetNullableVec callback");
        waiting--;
        if (id == 1)
        {
            ValidateNullableVecView(ctx, hasPos, x, y);
        }
        else
        {
            ValidateNullableVecView(ctx);
        }
    };

    conn.Reducers.OnInsertEmptyStringIntoNonNullable += (ReducerEventContext ctx) =>
    {
        Log.Info("Got InsertEmptyStringIntoNonNullable callback");
        waiting--;
        Debug.Assert(
            ctx.Event.Status is Status.Committed,
            $"InsertEmptyStringIntoNonNullable should commit, got {ctx.Event.Status}"
        );
        Debug.Assert(
            ctx.Db.NullStringNonnullable.Iter().Any(r => r.Name == ""),
            "Expected a row inserted into null_string_nonnullable with Name == \"\""
        );
    };

    conn.Reducers.OnInsertNullStringIntoNonNullable += (ReducerEventContext ctx) =>
    {
        Log.Info("Got InsertNullStringIntoNonNullable callback");
        waiting--;

        if (ctx.Event.Status is Status.Failed(var reason))
        {
            Debug.Assert(
                reason.Contains(
                    "Cannot serialize a null string",
                    StringComparison.OrdinalIgnoreCase
                )
                    || reason.Contains("BSATN", StringComparison.OrdinalIgnoreCase)
                    || reason.Contains("nullable string", StringComparison.OrdinalIgnoreCase),
                $"Expected a serialization-related failure message, got: {reason}"
            );
        }
        else
        {
            throw new Exception(
                $"InsertNullStringIntoNonNullable should fail, got status {ctx.Event.Status}"
            );
        }
    };

    conn.Reducers.OnInsertNullStringIntoNullable += (ReducerEventContext ctx) =>
    {
        Log.Info("Got InsertNullStringIntoNullable callback");
        waiting--;
        Debug.Assert(
            ctx.Event.Status is Status.Committed,
            $"InsertNullStringIntoNullable should commit, got {ctx.Event.Status}"
        );
        Debug.Assert(
            ctx.Db.NullStringNullable.Iter().Any(r => r.Name == null),
            "Expected a row inserted into null_string_nullable with Name == null"
        );
    };

    conn.Reducers.OnUpdateWhereTest += (
        ReducerEventContext ctx,
        uint id,
        uint value,
        string name
    ) =>
    {
        Log.Info("Got UpdateWhereTest callback");
        waiting--;
        Debug.Assert(id == 2, $"Expected UpdateWhereTest to target id=2, got {id}");
        Debug.Assert(
            value == UPDATED_WHERE_TEST_VALUE,
            $"Expected UpdateWhereTest value {UPDATED_WHERE_TEST_VALUE}, got {value}"
        );
        Debug.Assert(
            name == UPDATED_WHERE_TEST_NAME,
            $"Expected UpdateWhereTest name {UPDATED_WHERE_TEST_NAME}, got {name}"
        );

        ValidateWhereSubscription(ctx, UPDATED_WHERE_TEST_NAME);
        ValidateWhereTestViews(ctx, UPDATED_WHERE_TEST_VALUE, UPDATED_WHERE_TEST_NAME);
    };

    conn.Db.TestEvent.OnInsert += (EventContext ctx, TestEvent row) =>
    {
        Log.Info($"Got TestEvent.OnInsert callback: {row.Name} / {row.Value}");
        testEventInsertCount++;
        Debug.Assert(
            row.Name == EXPECTED_TEST_EVENT_NAME,
            $"Expected TestEvent.Name == {EXPECTED_TEST_EVENT_NAME}, got {row.Name}"
        );
        Debug.Assert(
            row.Value == EXPECTED_TEST_EVENT_VALUE,
            $"Expected TestEvent.Value == {EXPECTED_TEST_EVENT_VALUE}, got {row.Value}"
        );
        Debug.Assert(
            ctx.Db.TestEvent.Count == 0,
            $"Event table should not persist rows. Count was {ctx.Db.TestEvent.Count}"
        );
        Debug.Assert(
            !ctx.Db.TestEvent.Iter().Any(),
            "Event table iterator should be empty after event delivery"
        );
    };

    conn.Reducers.OnEmitTestEvent += (ReducerEventContext ctx, string name, ulong value) =>
    {
        Log.Info("Got EmitTestEvent callback");
        waiting--;
        Debug.Assert(
            ctx.Event.Status is Status.Committed,
            $"EmitTestEvent should commit, got {ctx.Event.Status}"
        );
        Debug.Assert(name == EXPECTED_TEST_EVENT_NAME, $"Expected name={EXPECTED_TEST_EVENT_NAME}, got {name}");
        Debug.Assert(value == EXPECTED_TEST_EVENT_VALUE, $"Expected value={EXPECTED_TEST_EVENT_VALUE}, got {value}");
    };

    conn.Reducers.OnNoop += (ReducerEventContext ctx) =>
    {
        Log.Info("Got Noop callback");
        waiting--;
        Debug.Assert(
            testEventInsertCount == 1,
            $"Expected exactly one TestEvent insert callback after noop, got {testEventInsertCount}"
        );
        Debug.Assert(
            ctx.Db.TestEvent.Count == 0,
            $"Event table should still be empty after noop. Count was {ctx.Db.TestEvent.Count}"
        );
        Debug.Assert(
            !ctx.Db.TestEvent.Iter().Any(),
            "Event table iterator should remain empty after noop"
        );
    };
}

const uint MAX_ID = 10;

// Test that indexes remain in sync with the expected table state when deletes are received.
// This used to fail, when row types did not correctly implement IEquatable.
void ValidateBTreeIndexes(IRemoteDbContext conn)
{
    Log.Debug("Checking indexes...");
    foreach (var data in conn.Db.ExampleData.Iter())
    {
        Debug.Assert(
            conn.Db.ExampleData.ExampleDataIndexedIdxBtree.Filter(data.Id).Contains(data)
        );
    }
    var outOfIndex = conn.Db.ExampleData.Iter().ToHashSet();

    for (uint i = 0; i < MAX_ID; i++)
    {
        foreach (var data in conn.Db.ExampleData.ExampleDataIndexedIdxBtree.Filter(i))
        {
            Debug.Assert(outOfIndex.Contains(data));
        }
    }
    Log.Debug("   Indexes are good.");
}

void ValidateNullableVecView(
    IRemoteDbContext conn,
    bool? expectedHasPos = null,
    int expectedX = 0,
    int expectedY = 0
)
{
    Log.Debug("Checking nullable vec view...");
    Debug.Assert(conn.Db.NullableVecView != null, "conn.Db.NullableVecView != null");
    Debug.Assert(
        conn.Db.NullableVecView.Count >= 2,
        $"conn.Db.NullableVecView.Count = {conn.Db.NullableVecView.Count}"
    );

    var rows = conn.Db.NullableVecView.Iter().ToList();
    Debug.Assert(rows.Any(r => r.Id == 1));
    Debug.Assert(rows.Any(r => r.Id == 2));

    var remoteRows = conn.Db.NullableVecView.RemoteQuery("WHERE Id = 1").Result;
    Debug.Assert(remoteRows != null && remoteRows.Length == 1);
    Debug.Assert(remoteRows[0].Id == 1);

    if (expectedHasPos is bool hasPos)
    {
        var row1 = rows.First(r => r.Id == 1);
        if (!hasPos)
        {
            Debug.Assert(row1.Pos == null, "Expected NullableVecView row 1 Pos == null");
        }
        else
        {
            Debug.Assert(row1.Pos != null, "Expected NullableVecView row 1 Pos != null");
            Debug.Assert(
                row1.Pos.X == expectedX,
                $"Expected row1.Pos.X == {expectedX}, got {row1.Pos.X}"
            );
            Debug.Assert(
                row1.Pos.Y == expectedY,
                $"Expected row1.Pos.Y == {expectedY}, got {row1.Pos.Y}"
            );
        }
    }
}

void ValidateReducerErrorDoesNotContainStackTrace(Exception exception)
{
    Debug.Assert(
        exception.Message == THROW_ERROR_MESSAGE,
        $"Expected reducer error message '{THROW_ERROR_MESSAGE}', got '{exception.Message}'"
    );
    Debug.Assert(
        !exception.Message.Contains("\n"),
        "Reducer error message should not contain newline"
    );
    Debug.Assert(
        !exception.Message.Contains("\r"),
        "Reducer error message should not contain newline"
    );
    Debug.Assert(
        !exception.Message.Contains(" at "),
        "Reducer error message should not contain stack trace"
    );
}

void ValidateQueryingWithIndexesExamples(IRemoteDbContext conn)
{
    Log.Debug("Checking 'Querying with Indexes' documentation examples...");

    var usersNamedAlice = conn.Db.UsersNamedAlice.Iter().Select(u => u.Name).ToList();
    Debug.Assert(
        usersNamedAlice.Count == 1 && usersNamedAlice[0] == "Alice",
        "Expected exactly one Alice in users_named_alice view"
    );

    var ages18To65 = conn.Db.UsersAge1865.Iter().Select(u => u.Name).ToHashSet();
    Debug.Assert(
        ages18To65.SetEquals(new[] { "Alice", "Charlie" }),
        "Expected Alice and Charlie in 18-65 age range"
    );

    var ages18OrOlder = conn.Db.UsersAge18Plus.Iter().Select(u => u.Name).ToHashSet();
    Debug.Assert(
        ages18OrOlder.SetEquals(new[] { "Alice", "Charlie" }),
        "Expected Alice and Charlie to be >= 18"
    );

    var youngerThan18 = conn.Db.UsersAgeUnder18.Iter().Select(u => u.Name).ToHashSet();
    Debug.Assert(youngerThan18.SetEquals(new[] { "Bob" }), "Expected Bob to be the only minor");

    var player123Scores = conn.Db.ScoresPlayer123.Iter().ToList();
    Debug.Assert(
        player123Scores.Count == 3,
        $"Expected 3 scores for player 123, got {player123Scores.Count}"
    );

    var player123LevelRange = conn.Db.ScoresPlayer123Range.Iter().ToList();
    Debug.Assert(
        player123LevelRange.Count == 3,
        "Expected three scores for player 123 between levels 1 and 10 inclusive"
    );

    var player123Level5 = conn.Db.ScoresPlayer123Level5.Iter().ToList();
    Debug.Assert(
        player123Level5.Count == 1 && player123Level5[0].Points == 5_000,
        "Expected a single level-5 score worth 5,000 points for player 123"
    );

    Log.Debug("Checking advanced typed query builder predicates...");
    Debug.Assert(conn.Db.User != null, "conn.Db.User should not be null");
    var advancedUsers = conn.Db.User.Iter().ToList();
    Debug.Assert(
        advancedUsers.Count == 2,
        $"Expected 2 rows from advanced user predicate, got {advancedUsers.Count}"
    );
    Debug.Assert(
        advancedUsers.All(u => u.Age >= 18 && u.Age < 65),
        "Advanced predicate rows should have 18 <= age < 65"
    );
    Debug.Assert(
        advancedUsers.All(u => u.IsAdmin || u.Name == "Charlie"),
        "Advanced predicate rows should satisfy admin || name == Charlie"
    );
    Debug.Assert(
        advancedUsers.Select(u => u.Name).OrderBy(n => n).SequenceEqual(new[] { "Alice", "Charlie" }),
        "Expected Alice and Charlie from advanced predicate"
    );
}

void ValidateWhereSubscription(IRemoteDbContext conn, string expectedTestName = "this_name_will_get_updated")
{
    Log.Debug("Checking typed WHERE subscription...");
    Debug.Assert(conn.Db.WhereTest != null, "conn.Db.WhereTest != null");

    var rows = conn.Db.WhereTest.Iter().ToList();
    Debug.Assert(rows.Count == 2, $"Expected 2 where_test rows, got {rows.Count}");
    Debug.Assert(rows.All(r => r.Value > 10), "Expected all where_test.Value > 10");
    Debug.Assert(
        rows.Any(r => r.Id == 2 && r.Name == expectedTestName),
        $"Expected where_test row id=2 name={expectedTestName}"
    );
    Debug.Assert(
        rows.Any(r => r.Id == 3 && r.Name == "this_name_will_not_be_updated"),
        "Expected where_test row id=3 name=this_name_will_not_be_updated"
    );
}

void ValidateWhereTestViews(
    IRemoteDbContext conn,
    uint expectedId2Value = 15,
    string expectedId2Name = "this_name_will_get_updated"
)
{
    Log.Debug("Checking where_test views...");
    Debug.Assert(
        conn.Db.WhereTestView != null,
        "WhereTestView should not be null"
    );
    Debug.Assert(
        conn.Db.WhereTestView.Count == 1,
        $"Expected exactly one WhereTestView row, got {conn.Db.WhereTestView.Count}"
    );
    var viewRow = conn.Db.WhereTestView.Iter().First();
    Debug.Assert(viewRow.Id == 2, $"Expected WhereTestView row id=2, got {viewRow.Id}");
    Debug.Assert(
        viewRow.Value == expectedId2Value,
        $"Expected WhereTestView row value={expectedId2Value}, got {viewRow.Value}"
    );
    Debug.Assert(
        viewRow.Name == expectedId2Name,
        $"Expected WhereTestView row name={expectedId2Name}, got {viewRow.Name}"
    );

    Debug.Assert(
        conn.Db.WhereTestQuery != null,
        "WhereTestQuery should not be null"
    );
    Debug.Assert(
        conn.Db.WhereTestQuery.Count == 1,
        $"Expected exactly one WhereTestQuery row, got {conn.Db.WhereTestQuery.Count}"
    );
    var queryRow = conn.Db.WhereTestQuery.Iter().First();
    Debug.Assert(queryRow.Id == 2, $"Expected WhereTestQuery row id=2, got {queryRow.Id}");
    Debug.Assert(
        queryRow.Value == expectedId2Value,
        $"Expected WhereTestQuery row value={expectedId2Value}, got {queryRow.Value}"
    );
    Debug.Assert(
        queryRow.Name == expectedId2Name,
        $"Expected WhereTestQuery row name={expectedId2Name}, got {queryRow.Name}"
    );

    Debug.Assert(
        conn.Db.FindWhereTest != null,
        "FindWhereTest should not be null"
    );
    Debug.Assert(
        conn.Db.FindWhereTest.Count == 1,
        $"Expected exactly one FindWhereTest row, got {conn.Db.FindWhereTest.Count}"
    );
    var anonRow = conn.Db.FindWhereTest.Iter().First();
    Debug.Assert(anonRow.Id == 3, $"Expected FindWhereTest row id=3, got {anonRow.Id}");
}

void ValidateSemijoinSubscriptions(IRemoteDbContext conn, Identity identity)
{
    Log.Debug("Checking typed semijoin subscriptions...");

    var players = conn.Db.Player.Iter().ToList();
    Debug.Assert(players.Count == 1, $"Expected 1 player row, got {players.Count}");
    Debug.Assert(
        players[0].Identity == identity,
        "Expected player.Identity to match the connection identity"
    );
    Debug.Assert(
        players[0].Name == "NewPlayer",
        $"Expected player.Name == NewPlayer, got {players[0].Name}"
    );

    var playerId = players[0].Id;

    var levels = conn.Db.PlayerLevel.Iter().ToList();
    Debug.Assert(levels.Count == 1, $"Expected 1 player_level row, got {levels.Count}");
    Debug.Assert(
        levels[0].PlayerId == playerId,
        "Expected player_level.PlayerId to match the subscribed player id"
    );
    Debug.Assert(levels[0].Level == 1, $"Expected player_level.Level == 1, got {levels[0].Level}");
}

void OnSubscriptionApplied(SubscriptionEventContext context)
{
    applied = true;

    ValidateWhereSubscription(context);
    ValidateWhereTestViews(context);
    ValidateSemijoinSubscriptions(context, context.Identity!.Value);

    // Do some operations that alter row state;
    // we will check that everything is in sync in the callbacks for these reducer calls.
    Log.Debug("Calling Add");
    waiting++;
    context.Reducers.Add(1, 1);

    Log.Debug("Calling Delete");
    waiting++;
    context.Reducers.Delete(1);

    Log.Debug("Calling Add");
    waiting++;
    context.Reducers.Add(1, 1);

    Log.Debug("Updating where_test row via reducer");
    waiting++;
    context.Reducers.UpdateWhereTest(2, UPDATED_WHERE_TEST_VALUE, UPDATED_WHERE_TEST_NAME);

    Log.Debug("Calling ThrowError");
    waiting++;
    context.Reducers.ThrowError(THROW_ERROR_MESSAGE);

    Log.Debug("Calling InsertResult");
    waiting++;
    context.Reducers.InsertResult(
        Result<MyTable, string>.Ok(new MyTable(new ReturnStruct(42, "magic")))
    );
    waiting++;
    context.Reducers.InsertResult(Result<MyTable, string>.Err("Fail"));

    Log.Debug("Calling RemoteQuery on my_log");
    var logRows = context.Db.MyLog.RemoteQuery("").Result;
    Debug.Assert(logRows != null && logRows.Length == 2);
    var logs = logRows.ToArray();
    var expected = new[]
    {
        new MyLog(Result<MyTable, string>.Ok(new MyTable(new ReturnStruct(42, "magic")))),
        new MyLog(Result<MyTable, string>.Err("Fail")),
    };
    Debug.Assert(logs.SequenceEqual(expected), "Logs did not match expected results");

    // RemoteQuery test
    Log.Debug("Calling RemoteQuery");
    // If testing against Rust, the query will need to be changed to "WHERE id = 0"
    var remoteRows = context.Db.ExampleData.RemoteQuery("WHERE Id = 1").Result;
    Debug.Assert(remoteRows != null && remoteRows.Length > 0);

    Log.Debug("Calling Admins.RemoteQuery");
    var remoteAdminRows = context.Db.Admins.RemoteQuery("WHERE IsAdmin = true").Result;
    Debug.Assert(remoteAdminRows != null && remoteAdminRows.Length > 0);

    // Views test
    Log.Debug("Checking Views are populated");
    Debug.Assert(context.Db.MyPlayer != null, "context.Db.MyPlayer != null");
    Debug.Assert(context.Db.MyAccount != null, "context.Db.MyAccount != null");
    Debug.Assert(context.Db.MyAccountMissing != null, "context.Db.MyAccountMissing != null");
    Debug.Assert(context.Db.PlayersAtLevelOne != null, "context.Db.PlayersAtLevelOne != null");
    Debug.Assert(
        context.Db.MyPlayer.Count > 0,
        $"context.Db.MyPlayer.Count = {context.Db.MyPlayer.Count}"
    );
    Debug.Assert(
        context.Db.MyAccount.Count == 1,
        $"context.Db.MyAccount.Count = {context.Db.MyAccount.Count}"
    );
    Debug.Assert(
        context.Db.MyAccountMissing.Count == 0,
        $"context.Db.MyAccountMissing.Count = {context.Db.MyAccountMissing.Count}"
    );
    Debug.Assert(
        context.Db.PlayersAtLevelOne.Count > 0,
        $"context.Db.PlayersAtLevelOne.Count = {context.Db.PlayersAtLevelOne.Count}"
    );
    Debug.Assert(context.Db.Admins != null, "context.Db.Admins != null");
    Debug.Assert(
        context.Db.Admins.Count > 0,
        $"context.Db.Admins.Count = {context.Db.Admins.Count}"
    );

    ValidateNullableVecView(context, expectedHasPos: true, expectedX: 1, expectedY: 2);

    ValidateQueryingWithIndexesExamples(context);

    Log.Debug("Calling Iter on View");
    var viewIterRows = context.Db.MyPlayer.Iter();
    var expectedPlayer = new Player
    {
        Id = 1,
        Identity = context.Identity!.Value,
        Name = "NewPlayer",
    };
    Log.Debug(
        "MyPlayer Iter count: " + (viewIterRows != null ? viewIterRows.Count().ToString() : "null")
    );
    Debug.Assert(viewIterRows != null && viewIterRows.Any());
    Log.Debug(
        "Validating View row data "
            + $"Id={expectedPlayer.Id}, Identity={expectedPlayer.Identity}, Name={expectedPlayer.Name} => "
            + $"Id={viewIterRows.First().Id}, Identity={viewIterRows.First().Identity}, Name={viewIterRows.First().Name}"
    );
    Debug.Assert(viewIterRows.First().Equals(expectedPlayer));

    Log.Debug("Calling Iter on View Admins");
    var adminsIterRows = context.Db.Admins.Iter();
    var expectedAdminNames = new HashSet<string> { "Alice", "Charlie" };
    Log.Debug(
        "Admins Iter count: "
            + (adminsIterRows != null ? adminsIterRows.Count().ToString() : "null")
    );
    Debug.Assert(adminsIterRows != null && adminsIterRows.Any());
    Log.Debug(
        "Validating Admins View row data "
            + $"Expected Names={string.Join(", ", expectedAdminNames)} => "
            + $"Actual Names={string.Join(", ", adminsIterRows.Select(a => a.Name))}"
    );
    Debug.Assert(adminsIterRows.All(a => expectedAdminNames.Contains(a.Name)));

    Log.Debug("Calling RemoteQuery on View");
    // If testing against Rust, the query will need to be changed to "WHERE id > 0"
    var viewRemoteQueryRows = context.Db.MyPlayer.RemoteQuery("WHERE Id > 0");
    Debug.Assert(viewRemoteQueryRows != null && viewRemoteQueryRows.Result.Length > 0);
    Debug.Assert(viewRemoteQueryRows.Result.First().Equals(expectedPlayer));

    Log.Debug("Calling Iter on Anonymous View");
    var anonViewIterRows = context.Db.PlayersAtLevelOne.Iter();
    var expectedPlayerAndLevel = new PlayerAndLevel
    {
        Id = 1,
        Identity = context.Identity!.Value,
        Name = "NewPlayer",
        Level = 1,
    };
    Log.Debug(
        "PlayersAtLevelOne Iter count: "
            + (anonViewIterRows != null ? anonViewIterRows.Count().ToString() : "null")
    );
    Debug.Assert(anonViewIterRows != null && anonViewIterRows.Any());
    Log.Debug(
        "Validating Anonymous View row data "
            + $"Id={expectedPlayerAndLevel.Id}, Identity={expectedPlayerAndLevel.Identity}, Name={expectedPlayerAndLevel.Name}, Level={expectedPlayerAndLevel.Level} => "
            + $"Id={anonViewIterRows.First().Id}, Identity={anonViewIterRows.First().Identity}, Name={anonViewIterRows.First().Name}, Level={anonViewIterRows.First().Level} => "
    );
    Debug.Assert(anonViewIterRows.First().Equals(expectedPlayerAndLevel));

    Log.Debug("Calling RemoteQuery on Anonymous View");
    // If testing against Rust, the query will need to be changed to "WHERE level = 1"
    var anonViewRemoteQueryRows = context.Db.PlayersAtLevelOne.RemoteQuery("WHERE Level = 1");
    Log.Debug(
        "PlayersAtLevelOne RemoteQuery count: "
            + (
                anonViewRemoteQueryRows != null
                    ? anonViewRemoteQueryRows.Result.Length.ToString()
                    : "null"
            )
    );
    Debug.Assert(anonViewRemoteQueryRows != null && anonViewRemoteQueryRows.Result.Length > 0);
    Debug.Assert(anonViewRemoteQueryRows.Result.First().Equals(expectedPlayerAndLevel));

    Log.Debug("Calling SetNullableVec (null)");
    waiting++;
    context.Reducers.SetNullableVec(1, false, 0, 0);

    Log.Debug("Calling SetNullableVec (some)");
    waiting++;
    context.Reducers.SetNullableVec(1, true, 7, 8);

    Log.Debug("Calling InsertEmptyStringIntoNonNullable");
    waiting++;
    context.Reducers.InsertEmptyStringIntoNonNullable();

    Log.Debug("Calling InsertNullStringIntoNonNullable (should fail)");
    waiting++;
    context.Reducers.InsertNullStringIntoNonNullable();

    Log.Debug("Calling InsertNullStringIntoNullable");
    waiting++;
    context.Reducers.InsertNullStringIntoNullable();

    Log.Debug("Calling EmitTestEvent");
    waiting++;
    context.Reducers.EmitTestEvent(EXPECTED_TEST_EVENT_NAME, EXPECTED_TEST_EVENT_VALUE);

    Log.Debug("Calling Noop after EmitTestEvent");
    waiting++;
    context.Reducers.Noop();

    // Procedures tests
    Log.Debug("Calling ReadMySchemaViaHttp");
    waiting++;
    context.Procedures.ReadMySchemaViaHttp(
        (IProcedureEventContext ctx, ProcedureCallbackResult<string> result) =>
        {
            try
            {
                Debug.Assert(
                    result.IsSuccess,
                    $"ReadMySchemaViaHttp should succeed. Error received: {result.Error}"
                );
                Debug.Assert(result.Value != null, "ReadMySchemaViaHttp should return a string");
                Debug.Assert(
                    result.Value.StartsWith("OK "),
                    $"Expected OK prefix, got: {result.Value}"
                );
                Debug.Assert(
                    result.Value.Contains("example_data"),
                    $"Expected schema response to mention example_data, got: {result.Value}"
                );
            }
            finally
            {
                waiting--;
            }
        }
    );

    Log.Debug("Calling InvalidHttpRequest");
    waiting++;
    context.Procedures.InvalidHttpRequest(
        (IProcedureEventContext ctx, ProcedureCallbackResult<string> result) =>
        {
            try
            {
                Debug.Assert(
                    result.IsSuccess,
                    $"InvalidHttpRequest should succeed. Error received: {result.Error}"
                );
                Debug.Assert(result.Value != null, "InvalidHttpRequest should return a string");
                Debug.Assert(
                    result.Value.StartsWith("ERR "),
                    $"Expected ERR prefix, got: {result.Value}"
                );
            }
            finally
            {
                waiting--;
            }
        }
    );

    Log.Debug("Calling InsertWithTxRollback");
    waiting++;
    context.Procedures.InsertWithTxRollback(
        (IProcedureEventContext ctx, ProcedureCallbackResult<SpacetimeDB.Unit> result) =>
        {
            if (result.IsSuccess)
            {
                Debug.Assert(
                    context.Db.MyTable.Count == 0,
                    $"MyTable should remain empty after rollback. Count was {context.Db.MyTable.Count}"
                );
                Log.Debug("Insert with transaction rollback succeeded");
            }
            else
            {
                throw new Exception(
                    "Expected InsertWithTransactionRollback to fail, but it succeeded"
                );
            }
            waiting--;
        }
    );

    Log.Debug("Calling InsertWithTxRollbackResult");
    waiting++;
    context.Procedures.InsertWithTxRollbackResult(
        (
            IProcedureEventContext ctx,
            ProcedureCallbackResult<Result<ReturnStruct, string>> result
        ) =>
        {
            if (result.IsSuccess)
            {
                Debug.Assert(
                    context.Db.MyTable.Count == 0,
                    $"MyTable should remain empty after rollback result. Count was {context.Db.MyTable.Count}"
                );
                Log.Debug("Insert with transaction result rollback succeeded");
            }
            else
            {
                throw new Exception(
                    "Expected InsertWithTxRollbackResult to fail, but it succeeded"
                );
            }
            waiting--;
        }
    );

    Log.Debug("Calling InsertWithTxPanic");
    waiting++;
    context.Procedures.InsertWithTxPanic(
        (IProcedureEventContext ctx, ProcedureCallbackResult<SpacetimeDB.Unit> result) =>
        {
            try
            {
                Debug.Assert(
                    result.IsSuccess,
                    $"InsertWithTxPanic should succeed (exception is caught). Error received: {result.Error}"
                );
                Debug.Assert(
                    context.Db.MyTable.Count == 0,
                    $"MyTable should remain empty after exception abort. Count was {context.Db.MyTable.Count}"
                );
            }
            finally
            {
                waiting--;
            }
        }
    );

    Log.Debug("Calling DanglingTxWarning");
    waiting++;
    context.Procedures.DanglingTxWarning(
        (IProcedureEventContext ctx, ProcedureCallbackResult<SpacetimeDB.Unit> result) =>
        {
            try
            {
                Debug.Assert(
                    result.IsSuccess,
                    $"DanglingTxWarning should succeed. Error received: {result.Error}"
                );
                Debug.Assert(
                    context.Db.MyTable.Count == 0,
                    $"MyTable should remain empty after dangling tx auto-abort. Count was {context.Db.MyTable.Count}"
                );
                // Note: We can't easily assert on the warning log from client-side,
                // but the server-side AssertRowCount verifies the auto-abort behavior
            }
            finally
            {
                waiting--;
            }
        }
    );

    Log.Debug("Calling InsertWithTxCommit");
    waiting++;
    context.Procedures.InsertWithTxCommit(
        (IProcedureEventContext ctx, ProcedureCallbackResult<SpacetimeDB.Unit> result) =>
        {
            try
            {
                Debug.Assert(
                    result.IsSuccess,
                    $"InsertWithTxCommit should succeed. Error received: {result.Error}"
                );
                var expectedRow = new MyTable(new ReturnStruct(42, "magic"));
                var row = context.Db.MyTable.Iter().FirstOrDefault();
                Debug.Assert(row != null);
                Debug.Assert(row.Equals(expectedRow));
                Log.Debug("Insert with transaction commit succeeded");
            }
            finally
            {
                waiting--;
            }
        }
    );

    Log.Debug("Calling InsertWithTxRetry");
    waiting++;
    context.Procedures.InsertWithTxRetry(
        (IProcedureEventContext ctx, ProcedureCallbackResult<SpacetimeDB.Unit> result) =>
        {
            try
            {
                Debug.Assert(
                    result.IsSuccess,
                    $"InsertWithTxRetry should succeed after retry. Error received: {result.Error}"
                );
            }
            catch (Exception ex)
            {
                Log.Exception(ex);
                throw;
            }
            finally
            {
                waiting--;
            }
        }
    );

    Log.Debug("Calling TxContextCapabilities");
    waiting++;
    context.Procedures.TxContextCapabilities(
        (IProcedureEventContext ctx, ProcedureCallbackResult<ReturnStruct> result) =>
        {
            try
            {
                Debug.Assert(
                    result.IsSuccess,
                    $"TxContextCapabilities should succeed. Error received: {result.Error}"
                );
                Debug.Assert(
                    result.Value != null && result.Value.B.StartsWith("sender:"),
                    $"Expected sender info, got {result.Value.B}"
                );

                // Verify the inserted row has the expected data
                var rows = context.Db.MyTable.Iter().ToList();
                var timestampRow = rows.FirstOrDefault(r => r.Field.B.StartsWith("tx-test"));
                Debug.Assert(
                    timestampRow != null && timestampRow.Field.A == 200,
                    $"Expected field.A == 200, got {timestampRow.Field.A}"
                );
                Debug.Assert(
                    timestampRow.Field.B == "tx-test",
                    $"Expected field.B == 'tx-test', got {timestampRow.Field.B}"
                );
            }
            finally
            {
                waiting--;
            }
        }
    );

    Log.Debug("Calling AuthenticationCapabilities");
    waiting++;
    context.Procedures.AuthenticationCapabilities(
        (IProcedureEventContext ctx, ProcedureCallbackResult<ReturnStruct> result) =>
        {
            try
            {
                Debug.Assert(
                    result.IsSuccess,
                    $"AuthenticationCapabilities should succeed. Error received: {result.Error}"
                );
                Debug.Assert(result.Value != null, "Should return a valid sender-derived value");
                Debug.Assert(
                    result.Value.B.Contains("jwt:") || result.Value.B == "no-jwt",
                    $"Should return JWT info, got {result.Value.B}"
                );

                // Verify the inserted row has authentication information
                var rows = context.Db.MyTable.Iter().ToList();

                var authRow = rows.FirstOrDefault(r => r.Field.B.StartsWith("auth:"));
                Debug.Assert(authRow is not null, "Should have a row with auth data");
                Debug.Assert(
                    authRow.Field.B.Contains("sender:"),
                    "Auth row should contain sender info"
                );
                Debug.Assert(
                    authRow.Field.B.Contains("conn:"),
                    "Auth row should contain connection info"
                );
            }
            finally
            {
                waiting--;
            }
        }
    );

    Log.Debug("Calling SubscriptionEventOffset");
    waiting++;
    context.Procedures.SubscriptionEventOffset(
        (IProcedureEventContext ctx, ProcedureCallbackResult<ReturnStruct> result) =>
        {
            try
            {
                Debug.Assert(
                    result.IsSuccess,
                    $"SubscriptionEventOffset should succeed. Error received: {result.Error}"
                );
                Debug.Assert(
                    result.Value != null && result.Value.A == 999,
                    $"Expected A == 999, got {result.Value.A}"
                );
                Debug.Assert(
                    result.Value.B.StartsWith("committed:"),
                    $"Expected committed timestamp, got {result.Value.B}"
                );

                // Verify the inserted row has the expected offset test data
                var rows = context.Db.MyTable.Iter().ToList();
                var offsetRow = rows.FirstOrDefault(r => r.Field.B.StartsWith("offset-test:"));
                Debug.Assert(offsetRow is not null, "Should have a row with offset-test data");
                Debug.Assert(offsetRow.Field.A == 999, "Offset test row should have A == 999");

                // Note: Transaction offset information is not directly accessible in ProcedureEvent,
                // but this test verifies that the transaction was committed and subscription events were generated
                // The presence of the new row in the subscription confirms the transaction offset was processed
            }
            finally
            {
                waiting--;
            }
        }
    );

    Log.Debug("Calling DocumentationGapChecks with valid parameters");
    waiting++;
    context.Procedures.DocumentationGapChecks(
        42,
        "test-input",
        (IProcedureEventContext ctx, ProcedureCallbackResult<ReturnStruct> result) =>
        {
            try
            {
                Debug.Assert(
                    result.IsSuccess,
                    "DocumentationGapChecks should succeed with valid parameters"
                );

                // Expected: inputValue * 2 + inputText.Length = 42 * 2 + 10 = 94
                var expectedValue = 42u * 2 + (uint)"test-input".Length; // 84 + 10 = 94
                Debug.Assert(
                    result.Value != null && result.Value.A == expectedValue,
                    $"Expected A == {expectedValue}, got {result.Value.A}"
                );
                Debug.Assert(
                    result.Value.B.StartsWith("success:"),
                    $"Expected success message, got {result.Value.B}"
                );
                Debug.Assert(
                    result.Value.B.Contains("test-input"),
                    "Result should contain input text"
                );

                // Verify the inserted row has the expected documentation gap test data
                var rows = context.Db.MyTable.Iter().ToList();
                var docGapRow = rows.FirstOrDefault(r => r.Field.B.StartsWith("doc-gap:"));
                Debug.Assert(docGapRow is not null, "Should have a row with doc-gap data");
                Debug.Assert(
                    docGapRow.Field.A == expectedValue,
                    $"Doc gap row should have A == {expectedValue}"
                );
                Debug.Assert(
                    docGapRow.Field.B.Contains("test-input"),
                    "Doc gap row should contain input text"
                );
            }
            finally
            {
                waiting--;
            }
        }
    );

    // Test error handling with invalid parameters
    Log.Debug("Calling DocumentationGapChecks with invalid parameters (should fail)");
    waiting++;
    context.Procedures.DocumentationGapChecks(
        0,
        "",
        (IProcedureEventContext ctx, ProcedureCallbackResult<ReturnStruct> result) =>
        {
            try
            {
                Debug.Assert(
                    !result.IsSuccess,
                    "DocumentationGapChecks should fail with invalid parameters"
                );
                // TODO: Testing against Rust, this returned a different error type "System.Exception". Decide if this is a bug or not.
                //Debug.Assert(result.Error is ArgumentException, $"Expected ArgumentException, got {result.Error?.GetType()}");
            }
            finally
            {
                waiting--;
            }
        }
    );

    // Now unsubscribe and check that the unsubscribing is actually applied.
    Log.Debug("Calling Unsubscribe");
    waiting++;
    handle?.UnsubscribeThen(
        (ctx) =>
        {
            Log.Debug("Received Unsubscribe");
            ValidateBTreeIndexes(ctx);
            waiting--;
        }
    );
}

System.AppDomain.CurrentDomain.UnhandledException += (sender, args) =>
{
    Log.Exception($"Unhandled exception: {sender} {args}");
    Environment.Exit(1);
};
var db = ConnectToDB();
Log.Info("Starting timer");
const int TIMEOUT = 20; // seconds;
var start = DateTime.Now;
while (!applied || waiting > 0)
{
    db.FrameTick();
    Thread.Sleep(100);
    if ((DateTime.Now - start).Seconds > TIMEOUT)
    {
        Log.Error($"Timeout, all events should have elapsed in {TIMEOUT} seconds!");
        Environment.Exit(1);
    }
}
Log.Info("Success");
Environment.Exit(0);
