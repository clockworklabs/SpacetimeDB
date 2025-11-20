/// Regression tests run with a live server.
/// To run these, run a local SpacetimeDB via `spacetime start`,
/// then in a separate terminal run `tools~/run-regression-tests.sh PATH_TO_SPACETIMEDB_REPO_CHECKOUT`.
/// This is done on CI in .github/workflows/test.yml.

using System.Diagnostics;
using System.Runtime.CompilerServices;
using SpacetimeDB;
using SpacetimeDB.Types;

const string HOST = "http://localhost:3000";
const string DBNAME = "btree-repro";

DbConnection ConnectToDB()
{
    DbConnection? conn = null;
    conn = DbConnection.Builder()
        .WithUri(HOST)
        .WithModuleName(DBNAME)
        .OnConnect(OnConnected)
        .OnConnectError((err) =>
        {
            throw err;
        })
        .OnDisconnect((conn, err) =>
        {
            if (err != null)
            {
                throw err;
            }
            else
            {
                throw new Exception("Unexpected disconnect");
            }
        })
        .Build();
    return conn;
}

uint waiting = 0;
var applied = false;
SubscriptionHandle? handle = null;

void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    Log.Debug($"Connected to {DBNAME} on {HOST}");
    handle = conn.SubscriptionBuilder()
        .OnApplied(OnSubscriptionApplied)
        .OnError((ctx, err) =>
        {
            throw err;
        })
        .Subscribe(["SELECT * FROM ExampleData", "SELECT * FROM MyPlayer", "SELECT * FROM PlayersForLevel"]);

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

    conn.OnUnhandledReducerError += (ReducerEventContext ctx, Exception exception) =>
    {
        Log.Info($"Got OnUnhandledReducerError: {exception}");
        waiting--;
        ValidateBTreeIndexes(ctx);
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
        Debug.Assert(conn.Db.ExampleData.Indexed.Filter(data.Id).Contains(data));
    }
    var outOfIndex = conn.Db.ExampleData.Iter().ToHashSet();

    for (uint i = 0; i < MAX_ID; i++)
    {
        foreach (var data in conn.Db.ExampleData.Indexed.Filter(i))
        {
            Debug.Assert(outOfIndex.Contains(data));
        }
    }
    Log.Debug("   Indexes are good.");
}

void OnSubscriptionApplied(SubscriptionEventContext context)
{
    applied = true;

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

    Log.Debug("Calling ThrowError");
    waiting++;
    context.Reducers.ThrowError("this is an error");

    // RemoteQuery test
    Log.Debug("Calling RemoteQuery");
    var remoteRows = context.Db.ExampleData.RemoteQuery("WHERE Id = 1").Result;
    Debug.Assert(remoteRows != null && remoteRows.Length > 0);

    // Now unsubscribe and check that the unsubscribe is actually applied.
    Log.Debug("Calling Unsubscribe");
    waiting++;
    handle?.UnsubscribeThen((ctx) =>
    {
        Log.Debug("Received Unsubscribe");
        ValidateBTreeIndexes(ctx);
        waiting--;
    });


    // Views test

    Log.Debug("Checking Views are populated");
    Debug.Assert(context.Db.MyPlayer != null, "context.Db.MyPlayer != null");
    Debug.Assert(context.Db.PlayersForLevel != null, "context.Db.PlayersForLevel != null");
    Debug.Assert(context.Db.MyPlayer.Count > 0, $"context.Db.MyPlayer.Count = {context.Db.MyPlayer.Count}");
    Debug.Assert(context.Db.PlayersForLevel.Count > 0, $"context.Db.PlayersForLevel.Count = {context.Db.PlayersForLevel.Count}");

    Log.Debug("Calling Iter on View");
    var viewIterRows = context.Db.MyPlayer.Iter();
    var expectedPlayer = new Player { Id = 1, Identity = context.Identity!.Value, Name = "NewPlayer" };
    Log.Debug("MyPlayer Iter count: " + (viewIterRows != null ? viewIterRows.Count().ToString() : "null"));
    Debug.Assert(viewIterRows != null && viewIterRows.Any());
    Log.Debug("Validating View row data " +
              $"Id={expectedPlayer.Id}, Identity={expectedPlayer.Identity}, Name={expectedPlayer.Name} => " +
              $"Id={viewIterRows.First().Id}, Identity={viewIterRows.First().Identity}, Name={viewIterRows.First().Name}");
    Debug.Assert(viewIterRows.First().Id == expectedPlayer.Id &&
                 viewIterRows.First().Identity == expectedPlayer.Identity &&
                 viewIterRows.First().Name == expectedPlayer.Name);

    Log.Debug("Calling RemoteQuery on View");
    var viewRemoteQueryRows = context.Db.MyPlayer.RemoteQuery("WHERE Id > 0");
    Debug.Assert(viewRemoteQueryRows != null && viewRemoteQueryRows.Result.Length > 0);

    Log.Debug("Calling Iter on Anonymous View");
    var anonViewIterRows = context.Db.PlayersForLevel.Iter();
    Log.Debug("PlayersForLevel Iter count: " + (anonViewIterRows != null ? anonViewIterRows.Count().ToString() : "null"));
    Debug.Assert(anonViewIterRows != null && anonViewIterRows.Any());

    Log.Debug("Calling RemoteQuery on Anonymous View");
    var anonViewRemoteQueryRows = context.Db.PlayersForLevel.RemoteQuery("WHERE Level = 1");
    Log.Debug("PlayersForLevel RemoteQuery count: " + (anonViewRemoteQueryRows != null ? anonViewRemoteQueryRows.Result.Length.ToString() : "null"));
    Debug.Assert(anonViewRemoteQueryRows != null && anonViewRemoteQueryRows.Result.Length > 0);
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