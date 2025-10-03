/// Regression tests run with a live server.
/// To run these, run a local SpacetimeDB via `spacetime start`,
/// then in a separate terminal run `tools~/run-regression-tests.sh PATH_TO_SPACETIMEDB_REPO_CHECKOUT`.
/// This is done on CI in .github/workflows/test.yml.

using System.Diagnostics;
using System.Runtime.CompilerServices;
using SpacetimeDB;
using SpacetimeDB.Types;

const string HOST = "http://localhost:3000";
const string DBNAME = "republish-test";

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
}

void OnSubscriptionApplied(SubscriptionEventContext context)
{
    applied = true;

    // Do some operations that alter row state;
    // we will check that everything is in sync in the callbacks for these reducer calls.
    var TOLERANCE = 0.00001f;
    foreach (var exampleData in context.Db.ExampleData.Iter())
    {
        if (exampleData.TestPass == 1)
        {
            List<string> errors = new List<string>();
            // This row should have had values set by default Attributes
            if (exampleData.DefaultString != "This is a default string") { errors.Add("DefaultString"); }
            if (exampleData.DefaultBool != true) { errors.Add("DefaultBool"); }
            if (exampleData.DefaultI8 != 2) { errors.Add("DefaultI8"); }
            if (exampleData.DefaultU8 != 2) { errors.Add("DefaultU8"); }
            if (exampleData.DefaultI16 != 2) { errors.Add("DefaultI16"); }
            if (exampleData.DefaultU16 != 2) { errors.Add("DefaultU16"); }
            if (exampleData.DefaultI32 != 2) { errors.Add("DefaultI32"); }
            if (exampleData.DefaultU32 != 2) { errors.Add("DefaultU32"); }
            if (exampleData.DefaultI64 != 2) { errors.Add("DefaultI64"); }
            if (exampleData.DefaultU64 != 2) { errors.Add("DefaultU64"); }
            if (exampleData.DefaultHex != 2) { errors.Add("DefaultHex"); }
            if (exampleData.DefaultBin != 2) { errors.Add("DefaultBin"); }
            if (Math.Abs(exampleData.DefaultF32 - 2.0f) > TOLERANCE) { errors.Add("DefaultF32"); }
            if (Math.Abs(exampleData.DefaultF64 - 2.0) > TOLERANCE) { errors.Add("DefaultF64"); }
            if (exampleData.DefaultEnum != MyEnum.SetByAttribute) { errors.Add("DefaultEnum"); }
            if (exampleData.DefaultNull != null) { errors.Add("DefaultNull"); }

            if (errors.Count > 0)
            {
                var errorString = string.Join(", ", errors);
                Log.Info($"ExampleData with key {exampleData.Primary}: Error: Key added during initial test pass, newly added rows {errorString} were not set by default attributes");
            }
            else
            {
                Log.Info($"ExampleData with key {exampleData.Primary}: Success! Key added during initial test pass, newly added rows are all properly set by default attributes");
            }
        }
        else if (exampleData.TestPass == 2)
        {
            List<string> errors = new List<string>();
            // This row should have had values set by initialized values
            if (exampleData.DefaultString != "") { errors.Add("DefaultString"); }
            if (exampleData.DefaultBool != false) { errors.Add("DefaultBool"); }
            if (exampleData.DefaultI8 != 1) { errors.Add("DefaultI8"); }
            if (exampleData.DefaultU8 != 1) { errors.Add("DefaultU8"); }
            if (exampleData.DefaultI16 != 1) { errors.Add("DefaultI16"); }
            if (exampleData.DefaultU16 != 1) { errors.Add("DefaultU16"); }
            if (exampleData.DefaultI32 != 1) { errors.Add("DefaultI32"); }
            if (exampleData.DefaultU32 != 1) { errors.Add("DefaultU32"); }
            if (exampleData.DefaultI64 != 1) { errors.Add("DefaultI64"); }
            if (exampleData.DefaultU64 != 1) { errors.Add("DefaultU64"); }
            if (exampleData.DefaultHex != 1) { errors.Add("DefaultHex"); }
            if (exampleData.DefaultBin != 1) { errors.Add("DefaultBin"); }
            if (Math.Abs(exampleData.DefaultF32 - 1.0f) > TOLERANCE) { errors.Add("DefaultF32"); }
            if (Math.Abs(exampleData.DefaultF64 - 1.0) > TOLERANCE) { errors.Add("DefaultF64"); }
            if (exampleData.DefaultEnum != MyEnum.SetByInitalization) { errors.Add("DefaultEnum"); }
            if (exampleData.DefaultNull == null || exampleData.DefaultNull.X != 1) { errors.Add("DefaultNull"); }

            if (errors.Count > 0)
            {
                var errorString = string.Join(", ", errors);
                Log.Info($"ExampleData with key {exampleData.Primary}: Error: Key added after republishing, newly added rows {errorString} were not set by initialized values");
            }
            else
            {
                Log.Error($"ExampleData with key {exampleData.Primary}: Success! Key added after republishing, newly added rows are all properly set by initialized values");
            }
        }
    }
    Log.Info("Evaluation of ExampleData in republishing test completed.");
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