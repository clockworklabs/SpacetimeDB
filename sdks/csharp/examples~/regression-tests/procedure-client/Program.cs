/// Procedure tests run with a live server.
/// To run these, run a local SpacetimeDB via `spacetime start`,
/// then in a separate terminal run `tools~/run-regression-tests.sh PATH_TO_SPACETIMEDB_REPO_CHECKOUT`.
/// This is done on CI in .github/workflows/test.yml.

using System.Diagnostics;
using System.Runtime.CompilerServices;
using SpacetimeDB;
using SpacetimeDB.Types;

const string HOST = "http://localhost:3000";
const string DBNAME = "procedure-tests";

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

void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    Log.Debug($"Connected to {DBNAME} on {HOST}");
    conn.SubscriptionBuilder()
        .OnApplied(OnSubscriptionApplied)
        .OnError((ctx, err) =>
        {
            throw err;
        })
        .SubscribeToAllTables();

    conn.OnUnhandledReducerError += (ReducerEventContext ctx, Exception exception) =>
    {
        Log.Info($"Got OnUnhandledReducerError: {exception}");
        waiting--;
    };
}


void OnSubscriptionApplied(SubscriptionEventContext context)
{
    applied = true;

    // Do some operations that alter row state;
    // we will check that everything is in sync in the callbacks for these reducer calls.
    Log.Debug("Calling return_primitive");
    waiting++;
    context.Procedures.ReturnPrimitive(42, 27, (ctx, result) =>
    {
        if (result.IsSuccess)
        {
            Debug.Assert(result.Value == 42+27);
            Log.Debug("return_primitive callback success");
        }
        else
        {
            throw result.Error!;
        }
        waiting--;
    });

    Log.Debug("Calling return_struct");
    waiting++;
    context.Procedures.ReturnStruct(42, "Hello, World!", (ctx, result) =>
    {
        if (result.IsSuccess)
        {
            Debug.Assert(result.Value!.A == 42 && result.Value!.B == "Hello, World!");
            Log.Debug("return_struct callback success");
        }
        else
        {
            throw result.Error!;
        }
        waiting--;
    });
    
    
    Log.Debug("Calling return_enum_a");
    waiting++;
    context.Procedures.ReturnEnumA(42, (ctx, result) =>
    {
        if (result.IsSuccess)
        {
            // result.Value is a ReturnEnum
            var extracted = result.Value switch
            {
                ReturnEnum.A(var aValue) => aValue,
                ReturnEnum.B(var bValue) => throw new Exception("Expected A variant but got B"),
                _ => throw new Exception("Unknown variant")
            };

            Debug.Assert(extracted == 42);
            Log.Debug("return_enum_a callback success");
        }
        else
        {
            throw result.Error!;
        }
        waiting--;
    });
    
    Log.Debug("Calling return_enum_b");
    waiting++;
    context.Procedures.ReturnEnumB("Hello, World!", (ctx, result) =>
    {
        if (result.IsSuccess)
        {
            // result.Value is a ReturnEnum
            var extracted = result.Value switch
            {
                ReturnEnum.B(var bValue) => bValue,
                ReturnEnum.A(var aValue) => throw new Exception("Expected B variant but got A"),
                _ => throw new Exception("Unknown variant")
            };

            Debug.Assert(extracted == "Hello, World!");
            Log.Debug("return_enum_b callback success");
        }
        else
        {
            throw result.Error!;
        }
        waiting--;
    });
    
    Log.Debug("Calling will_panic");
    waiting++;
    context.Procedures.WillPanic((ctx, result) =>
    {
        if (result.IsSuccess)
        {
            throw new Exception("Expected will_panic to fail, but it succeeded");
        }
        else
        {
            Log.Debug("will_panic callback received expected error");
        }
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