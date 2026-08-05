using System;
using System.Linq;
using System.Threading;
using SpacetimeDB;
using SpacetimeDB.Types;

const string DbNameEnvVar = "SPACETIME_SDK_TEST_DB_NAME";
const string ServerUrlEnvVar = "SPACETIME_SDK_TEST_SERVER_URL";

AppDomain.CurrentDomain.UnhandledException += (_, eventArgs) =>
{
    Console.Error.WriteLine(eventArgs.ExceptionObject);
    Environment.Exit(1);
};

var testName = args.Length > 0 ? args[0] : throw new ArgumentException("Pass a test name as argv[1]");
var dbName = Environment.GetEnvironmentVariable(DbNameEnvVar) ?? throw new InvalidOperationException($"{DbNameEnvVar} is not set");
var serverUrl = Environment.GetEnvironmentVariable(ServerUrlEnvVar) ?? "http://localhost:3000";

switch (testName)
{
    case "insert-primitive":
        RunInsertPrimitive();
        break;
    case "delete-primitive":
        RunDeletePrimitive();
        break;
    case "update-primitive":
        RunUpdatePrimitive();
        break;
    case "insert-builtin":
        RunInsertBuiltin();
        break;
    case "insert-vec":
        RunInsertVec();
        break;
    case "insert-option-some":
        RunInsertOptionSome();
        break;
    case "insert-option-none":
        RunInsertOptionNone();
        break;
    case "insert-struct":
        RunInsertStruct();
        break;
    case "insert-simple-enum":
        RunInsertSimpleEnum();
        break;
    case "insert-enum-with-payload":
        RunInsertEnumWithPayload();
        break;
    case "fail-reducer":
        RunFailReducer();
        break;
    case "caller-always-notified":
        RunCallerAlwaysNotified();
        break;
    default:
        throw new ArgumentException($"Unknown C# SDK harness test: {testName}");
}

void RunInsertPrimitive()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneU32()));
    var inserted = false;
    var reducerSeen = false;

    test.Db.Db.OneU32.OnInsert += (_, row) =>
    {
        Require(row.N == 123, $"Expected one_u32.n == 123, got {row.N}");
        inserted = true;
    };
    test.Db.Reducers.OnInsertOneU32 += (ctx, n) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(n == 123, $"Expected reducer arg 123, got {n}");
        reducerSeen = true;
    };

    test.Db.Reducers.InsertOneU32(123);
    test.FrameTickUntil(() => inserted && reducerSeen);
    Require(test.Db.Db.OneU32.Count == 1, $"Expected one_u32 cache count 1, got {test.Db.Db.OneU32.Count}");
}

void RunDeletePrimitive()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.PkU32()));
    var inserted = false;
    var deleted = false;

    test.Db.Db.PkU32.OnInsert += (_, row) =>
    {
        Require(row.N == 7 && row.Data == 10, "Unexpected pk_u32 insert row");
        inserted = true;
    };
    test.Db.Db.PkU32.OnDelete += (_, row) =>
    {
        Require(row.N == 7 && row.Data == 10, "Unexpected pk_u32 delete row");
        deleted = true;
    };

    test.Db.Reducers.InsertPkU32(7, 10);
    test.FrameTickUntil(() => inserted);
    test.Db.Reducers.DeletePkU32(7);
    test.FrameTickUntil(() => deleted);
    Require(test.Db.Db.PkU32.Count == 0, $"Expected pk_u32 cache count 0, got {test.Db.Db.PkU32.Count}");
}

void RunUpdatePrimitive()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.PkU32()));
    var inserted = false;
    var updated = false;

    test.Db.Db.PkU32.OnInsert += (_, row) =>
    {
        if (row.N == 9 && row.Data == 11)
        {
            inserted = true;
        }
    };
    test.Db.Db.PkU32.OnUpdate += (_, oldRow, newRow) =>
    {
        Require(oldRow.N == 9 && oldRow.Data == 11, "Unexpected old pk_u32 row");
        Require(newRow.N == 9 && newRow.Data == 12, "Unexpected new pk_u32 row");
        updated = true;
    };

    test.Db.Reducers.InsertPkU32(9, 11);
    test.FrameTickUntil(() => inserted);
    test.Db.Reducers.UpdatePkU32(9, 12);
    test.FrameTickUntil(() => updated);
    Require(test.Db.Db.PkU32.N.Find(9)?.Data == 12, "Updated row was not visible through primary-key index");
}

void RunInsertBuiltin()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneIdentity()).AddQuery(qb => qb.From.OneUuid()));
    var insertedIdentity = false;
    var insertedUuid = false;
    var uuid = Uuid.Parse("01890f3d-8120-7cc8-9a1f-cd1224fb3a10");

    test.Db.Db.OneIdentity.OnInsert += (_, row) =>
    {
        Require(row.I == test.Db.Identity, "Inserted identity did not match connection identity");
        insertedIdentity = true;
    };
    test.Db.Db.OneUuid.OnInsert += (_, row) =>
    {
        Require(row.U == uuid, "Inserted UUID did not round-trip");
        insertedUuid = true;
    };

    test.Db.Reducers.InsertCallerOneIdentity();
    test.Db.Reducers.InsertOneUuid(uuid);
    test.FrameTickUntil(() => insertedIdentity && insertedUuid);
}

void RunInsertVec()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.VecI32()).AddQuery(qb => qb.From.VecString()));
    var intsInserted = false;
    var stringsInserted = false;

    test.Db.Db.VecI32.OnInsert += (_, row) =>
    {
        Require(row.N.SequenceEqual(new[] { -1, 0, 42 }), "VecI32 did not round-trip");
        intsInserted = true;
    };
    test.Db.Db.VecString.OnInsert += (_, row) =>
    {
        Require(row.S.SequenceEqual(new[] { "alpha", "beta" }), "VecString did not round-trip");
        stringsInserted = true;
    };

    test.Db.Reducers.InsertVecI32(new() { -1, 0, 42 });
    test.Db.Reducers.InsertVecString(new() { "alpha", "beta" });
    test.FrameTickUntil(() => intsInserted && stringsInserted);
}

void RunInsertOptionSome()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OptionI32()).AddQuery(qb => qb.From.OptionString()));
    var intInserted = false;
    var stringInserted = false;

    test.Db.Db.OptionI32.OnInsert += (_, row) =>
    {
        Require(row.N == 42, "OptionI32 Some did not round-trip");
        intInserted = true;
    };
    test.Db.Db.OptionString.OnInsert += (_, row) =>
    {
        Require(row.S == "present", "OptionString Some did not round-trip");
        stringInserted = true;
    };

    test.Db.Reducers.InsertOptionI32(42);
    test.Db.Reducers.InsertOptionString("present");
    test.FrameTickUntil(() => intInserted && stringInserted);
}

void RunInsertOptionNone()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OptionI32()).AddQuery(qb => qb.From.OptionString()));
    var intInserted = false;
    var stringInserted = false;

    test.Db.Db.OptionI32.OnInsert += (_, row) =>
    {
        Require(row.N == null, "OptionI32 None did not round-trip");
        intInserted = true;
    };
    test.Db.Db.OptionString.OnInsert += (_, row) =>
    {
        Require(row.S == null, "OptionString None did not round-trip");
        stringInserted = true;
    };

    test.Db.Reducers.InsertOptionI32(null);
    test.Db.Reducers.InsertOptionString(null);
    test.FrameTickUntil(() => intInserted && stringInserted);
}

void RunInsertStruct()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneByteStruct()));
    var inserted = false;

    test.Db.Db.OneByteStruct.OnInsert += (_, row) =>
    {
        Require(row.S.B == 99, "ByteStruct did not round-trip");
        inserted = true;
    };

    test.Db.Reducers.InsertOneByteStruct(new ByteStruct { B = 99 });
    test.FrameTickUntil(() => inserted);
}

void RunInsertSimpleEnum()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneSimpleEnum()));
    var inserted = false;

    test.Db.Db.OneSimpleEnum.OnInsert += (_, row) =>
    {
        Require(row.E == SimpleEnum.Two, "SimpleEnum did not round-trip");
        inserted = true;
    };

    test.Db.Reducers.InsertOneSimpleEnum(SimpleEnum.Two);
    test.FrameTickUntil(() => inserted);
}

void RunInsertEnumWithPayload()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneEnumWithPayload()));
    var inserted = false;
    var payload = new EnumWithPayload.U8(17);

    test.Db.Db.OneEnumWithPayload.OnInsert += (_, row) =>
    {
        Require(row.E == payload, "EnumWithPayload did not round-trip");
        inserted = true;
    };

    test.Db.Reducers.InsertOneEnumWithPayload(payload);
    test.FrameTickUntil(() => inserted);
}

void RunFailReducer()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.UniqueU8()));
    var sawFailure = false;

    test.Db.Reducers.OnInsertUniqueU8 += (ctx, n, data) =>
    {
        if (ctx.Event.Status is Status.Failed)
        {
            Require(n == 1 && data == 20, "Unexpected failed insert_unique_u8 args");
            sawFailure = true;
        }
    };

    test.Db.Reducers.InsertUniqueU8(1, 10);
    test.FrameTickUntil(() => test.Db.Db.UniqueU8.Count == 1 || sawFailure);
    test.Db.Reducers.InsertUniqueU8(1, 20);
    test.FrameTickUntil(() => sawFailure);
}

void RunCallerAlwaysNotified()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneU32()));
    var callbackSeen = false;

    test.Db.Reducers.OnNoOpSucceeds += ctx =>
    {
        RequireCommitted(ctx.Event.Status);
        callbackSeen = true;
    };

    test.Db.Reducers.NoOpSucceeds();
    test.FrameTickUntil(() => callbackSeen);
    Require(test.Db.Db.OneU32.Count == 0, "No-op reducer unexpectedly mutated one_u32");
}

HarnessConnection ConnectAndSubscribe(Func<DbConnection, TypedSubscriptionBuilder> buildSubscription)
{
    DbConnection db = null!;
    var connected = false;
    var applied = false;

    db = DbConnection
        .Builder()
        .WithUri(serverUrl)
        .WithDatabaseName(dbName)
        .OnConnect((conn, identity, _) =>
        {
            Require(identity == conn.Identity, "Connection identity callback did not match connection state");
            buildSubscription(conn)
                .OnApplied(_ => applied = true)
                .OnError((_, err) => throw err)
                .Subscribe();
            connected = true;
        })
        .OnConnectError(err => throw err)
        .OnDisconnect((_, err) =>
        {
            if (err != null)
            {
                throw err;
            }

            throw new Exception("Connection disconnected unexpectedly");
        })
        .Build();

    var harness = new HarnessConnection(db);
    harness.FrameTickUntil(() => connected && applied);
    return harness;
}

void RequireCommitted(Status status)
{
    if (status is not Status.Committed)
    {
        throw new Exception($"Expected reducer to commit, got {status}");
    }
}

void Require(bool condition, string message)
{
    if (!condition)
    {
        throw new Exception(message);
    }
}

sealed class HarnessConnection : IDisposable
{
    public DbConnection Db { get; }

    public Identity Identity => Db.Identity ?? throw new InvalidOperationException("Connection has no identity yet");

    public HarnessConnection(DbConnection db)
    {
        Db = db;
    }

    public void FrameTickUntil(Func<bool> isComplete, int timeoutSeconds = 20)
    {
        var deadline = DateTime.UtcNow.AddSeconds(timeoutSeconds);
        while (!isComplete())
        {
            Db.FrameTick();
            Thread.Sleep(25);
            if (DateTime.UtcNow > deadline)
            {
                throw new TimeoutException($"Timed out after {timeoutSeconds} seconds");
            }
        }
    }

    public void Dispose()
    {
        Db.Disconnect();
    }
}
