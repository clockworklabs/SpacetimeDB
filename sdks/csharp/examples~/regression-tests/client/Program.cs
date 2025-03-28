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
bool applied = false;
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
        .Subscribe(["SELECT * FROM ExampleData"]);

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
    // Do some operations that alter row state;
    // we will check that everything is in sync in the callbacks for these reducer calls.
    Log.Debug("Calling Add");
    waiting++;
    context.Reducers.Add(1, 1);
    applied = true;

    Log.Debug("Calling Delete");
    waiting++;
    context.Reducers.Delete(1);

    Log.Debug("Calling Add");
    waiting++;
    context.Reducers.Add(1, 1);
    applied = true;

    // Now unsubscribe and check that the unsubscribe is actually applied.
    Log.Debug("Calling Unsubscribe");
    waiting++;
    handle?.UnsubscribeThen((ctx) =>
    {
        Log.Debug("Received Unsubscribe");
        ValidateBTreeIndexes(ctx);
        waiting--;
    });
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