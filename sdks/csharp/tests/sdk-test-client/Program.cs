using System;
using System.Collections.Generic;
using System.IO;
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
    case "subscribe-and-cancel":
        RunSubscribeAndCancel();
        break;
    case "subscribe-and-unsubscribe":
        RunSubscribeAndUnsubscribe();
        break;
    case "subscription-error-smoke-test":
        RunSubscriptionErrorSmokeTest();
        break;
    case "subscribe-all-select-star":
        RunSubscribeAllSelectStar();
        break;
    case "delete-primitive":
        RunDeletePrimitive();
        break;
    case "update-primitive":
        RunUpdatePrimitive();
        break;
    case "insert-identity":
        RunInsertIdentity();
        break;
    case "insert-caller-identity":
        RunInsertCallerIdentity();
        break;
    case "delete-identity":
        RunDeleteIdentity();
        break;
    case "update-identity":
        RunUpdateIdentity();
        break;
    case "insert-connection-id":
        RunInsertConnectionId();
        break;
    case "insert-caller-connection-id":
        RunInsertCallerConnectionId();
        break;
    case "delete-connection-id":
        RunDeleteConnectionId();
        break;
    case "update-connection-id":
        RunUpdateConnectionId();
        break;
    case "insert-timestamp":
        RunInsertTimestamp();
        break;
    case "insert-call-timestamp":
        RunInsertCallTimestamp();
        break;
    case "insert-uuid":
        RunInsertUuid();
        break;
    case "insert-call-uuid-v4":
        RunInsertCallUuidV4();
        break;
    case "insert-call-uuid-v7":
        RunInsertCallUuidV7();
        break;
    case "delete-uuid":
        RunDeleteUuid();
        break;
    case "update-uuid":
        RunUpdateUuid();
        break;
    case "on-reducer":
        RunOnReducer();
        break;
    case "fail-reducer":
        RunFailReducer();
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
    case "insert-delete-large-table":
        RunInsertDeleteLargeTable();
        break;
    case "insert-primitives-as-strings":
        RunInsertPrimitivesAsStrings();
        break;
    case "should-fail":
        throw new Exception("intentional failure for harness should_panic coverage");
    case "reauth":
        RunReauth();
        break;
    case "reconnect-different-connection-id":
        RunReconnectDifferentConnectionId();
        break;
    case "caller-always-notified":
        RunCallerAlwaysNotified();
        break;
    case "caller-alice-receives-reducer-callback-but-not-bob":
        RunCallerAliceReceivesReducerCallbackButNotBob();
        break;
    case "row-deduplication":
        RunRowDeduplication();
        break;
    case "row-deduplication-join-r-and-s":
        RunRowDeduplicationJoinRAndS();
        break;
    case "row-deduplication-r-join-s-and-r-joint":
        RunRowDeduplicationRJoinSAndRJoinT();
        break;
    case "test-lhs-join-update":
        RunLhsJoinUpdate(disjoint: false);
        break;
    case "test-lhs-join-update-disjoint-queries":
        RunLhsJoinUpdate(disjoint: true);
        break;
    case "test-intra-query-bag-semantics-for-join":
        RunIntraQueryBagSemanticsForJoin();
        break;
    case "two-different-compression-algos":
        RunTwoDifferentCompressionAlgos();
        break;
    case "test-parameterized-subscription":
        RunParameterizedSubscription();
        break;
    case "test-rls-subscription":
        RunRlsSubscription();
        break;
    case "pk-simple-enum":
        RunPkSimpleEnum();
        break;
    case "indexed-simple-enum":
        RunIndexedSimpleEnum();
        break;
    case "overlapping-subscriptions":
        RunOverlappingSubscriptions();
        break;
    case "sorted-uuids-insert":
        RunSortedUuidsInsert();
        break;
    default:
        throw new ArgumentException($"Unknown C# SDK harness test: {testName}");
}

void RunInsertPrimitive()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder()
        .AddQuery(qb => qb.From.OneU8())
        .AddQuery(qb => qb.From.OneU16())
        .AddQuery(qb => qb.From.OneU32())
        .AddQuery(qb => qb.From.OneU64())
        .AddQuery(qb => qb.From.OneI8())
        .AddQuery(qb => qb.From.OneI16())
        .AddQuery(qb => qb.From.OneI32())
        .AddQuery(qb => qb.From.OneI64())
        .AddQuery(qb => qb.From.OneBool())
        .AddQuery(qb => qb.From.OneF32())
        .AddQuery(qb => qb.From.OneF64())
        .AddQuery(qb => qb.From.OneString()));

    var remaining = 12;
    void Seen() => remaining--;

    test.Db.Db.OneU8.OnInsert += (_, row) => { Require(row.N == 1, "OneU8 did not round-trip"); Seen(); };
    test.Db.Db.OneU16.OnInsert += (_, row) => { Require(row.N == 2, "OneU16 did not round-trip"); Seen(); };
    test.Db.Db.OneU32.OnInsert += (_, row) => { Require(row.N == 3, "OneU32 did not round-trip"); Seen(); };
    test.Db.Db.OneU64.OnInsert += (_, row) => { Require(row.N == 4, "OneU64 did not round-trip"); Seen(); };
    test.Db.Db.OneI8.OnInsert += (_, row) => { Require(row.N == -1, "OneI8 did not round-trip"); Seen(); };
    test.Db.Db.OneI16.OnInsert += (_, row) => { Require(row.N == -2, "OneI16 did not round-trip"); Seen(); };
    test.Db.Db.OneI32.OnInsert += (_, row) => { Require(row.N == -3, "OneI32 did not round-trip"); Seen(); };
    test.Db.Db.OneI64.OnInsert += (_, row) => { Require(row.N == -4, "OneI64 did not round-trip"); Seen(); };
    test.Db.Db.OneBool.OnInsert += (_, row) => { Require(row.B, "OneBool did not round-trip"); Seen(); };
    test.Db.Db.OneF32.OnInsert += (_, row) => { Require(Math.Abs(row.F - 1.25f) < 0.001f, "OneF32 did not round-trip"); Seen(); };
    test.Db.Db.OneF64.OnInsert += (_, row) => { Require(Math.Abs(row.F - 2.5) < 0.001, "OneF64 did not round-trip"); Seen(); };
    test.Db.Db.OneString.OnInsert += (_, row) => { Require(row.S == "hello", "OneString did not round-trip"); Seen(); };

    test.Db.Reducers.InsertOneU8(1);
    test.Db.Reducers.InsertOneU16(2);
    test.Db.Reducers.InsertOneU32(3);
    test.Db.Reducers.InsertOneU64(4);
    test.Db.Reducers.InsertOneI8(-1);
    test.Db.Reducers.InsertOneI16(-2);
    test.Db.Reducers.InsertOneI32(-3);
    test.Db.Reducers.InsertOneI64(-4);
    test.Db.Reducers.InsertOneBool(true);
    test.Db.Reducers.InsertOneF32(1.25f);
    test.Db.Reducers.InsertOneF64(2.5);
    test.Db.Reducers.InsertOneString("hello");
    test.FrameTickUntil(() => remaining == 0);
}

void RunSubscribeAndCancel()
{
    using var test = Connect();
    var ended = false;
    var handle = test.Db.SubscriptionBuilder()
        .OnApplied(_ => throw new Exception("Subscription should never be applied"))
        .OnError((_, err) => throw err)
        .Subscribe(new[] { "SELECT * FROM one_u8" });

    Require(!handle.IsActive, "New subscription should not be active yet");
    Require(!handle.IsEnded, "New subscription should not be ended yet");
    handle.UnsubscribeThen(_ =>
    {
        Require(!handle.IsActive, "Canceled subscription should not be active");
        Require(handle.IsEnded, "Canceled subscription should be ended");
        ended = true;
    });
    test.FrameTickUntil(() => ended);
}

void RunSubscribeAndUnsubscribe()
{
    using var test = Connect();
    var ended = false;
    var applied = false;
    SubscriptionHandle? handle = null;

    test.Db.Reducers.InsertOneU8(1);
    handle = test.Db.SubscriptionBuilder()
        .OnApplied(ctx =>
        {
            applied = true;
            Require(handle is { IsActive: true, IsEnded: false }, "Applied subscription has wrong state");
            Require(ctx.Db.OneU8.Count == 1, "Expected one row after subscription applied");
            handle.UnsubscribeThen(endCtx =>
            {
                Require(endCtx.Db.OneU8.Count == 0, "Expected cache row to be removed by unsubscribe");
                ended = true;
            });
        })
        .OnError((_, err) => throw err)
        .Subscribe(new[] { "SELECT * FROM one_u8" });

    Require(!handle.IsActive, "New subscription should not be active yet");
    test.FrameTickUntil(() => applied && ended);
}

void RunSubscriptionErrorSmokeTest()
{
    using var test = Connect();
    var errored = false;
    var handle = test.Db.SubscriptionBuilder()
        .OnApplied(_ => throw new Exception("Invalid subscription unexpectedly applied"))
        .OnError((_, _) => errored = true)
        .Subscribe(new[] { "SELEcCT * FROM one_u8" });

    Require(!handle.IsActive, "Invalid subscription should not be active yet");
    test.FrameTickUntil(() => errored);
    Require(handle.IsEnded, "Invalid subscription handle did not end");
}

void RunSubscribeAllSelectStar()
{
    using var test = ConnectAndSubscribeAll();
    var remaining = 16;
    void Seen() => remaining--;

    var u128 = new U128(0, 5);
    var u256 = new U256(new U128(0, 0), new U128(0, 6));
    var i128 = new I128(0, 7);
    var i256 = new I256(new U128(0, 0), new U128(0, 8));

    test.Db.Db.OneU8.OnInsert += (_, row) => { Require(row.N == 1, "OneU8 did not round-trip"); Seen(); };
    test.Db.Db.OneU16.OnInsert += (_, row) => { Require(row.N == 2, "OneU16 did not round-trip"); Seen(); };
    test.Db.Db.OneU32.OnInsert += (_, row) => { Require(row.N == 3, "OneU32 did not round-trip"); Seen(); };
    test.Db.Db.OneU64.OnInsert += (_, row) => { Require(row.N == 4, "OneU64 did not round-trip"); Seen(); };
    test.Db.Db.OneU128.OnInsert += (_, row) => { Require(row.N.Equals(u128), "OneU128 did not round-trip"); Seen(); };
    test.Db.Db.OneU256.OnInsert += (_, row) => { Require(row.N.Equals(u256), "OneU256 did not round-trip"); Seen(); };
    test.Db.Db.OneI8.OnInsert += (_, row) => { Require(row.N == -1, "OneI8 did not round-trip"); Seen(); };
    test.Db.Db.OneI16.OnInsert += (_, row) => { Require(row.N == -2, "OneI16 did not round-trip"); Seen(); };
    test.Db.Db.OneI32.OnInsert += (_, row) => { Require(row.N == -3, "OneI32 did not round-trip"); Seen(); };
    test.Db.Db.OneI64.OnInsert += (_, row) => { Require(row.N == -4, "OneI64 did not round-trip"); Seen(); };
    test.Db.Db.OneI128.OnInsert += (_, row) => { Require(row.N.Equals(i128), "OneI128 did not round-trip"); Seen(); };
    test.Db.Db.OneI256.OnInsert += (_, row) => { Require(row.N.Equals(i256), "OneI256 did not round-trip"); Seen(); };
    test.Db.Db.OneBool.OnInsert += (_, row) => { Require(row.B, "OneBool did not round-trip"); Seen(); };
    test.Db.Db.OneF32.OnInsert += (_, row) => { Require(Math.Abs(row.F - 1.25f) < 0.001f, "OneF32 did not round-trip"); Seen(); };
    test.Db.Db.OneF64.OnInsert += (_, row) => { Require(Math.Abs(row.F - 2.5) < 0.001, "OneF64 did not round-trip"); Seen(); };
    test.Db.Db.OneString.OnInsert += (_, row) => { Require(row.S == "hello", "OneString did not round-trip"); Seen(); };

    test.Db.Reducers.InsertOneU8(1);
    test.Db.Reducers.InsertOneU16(2);
    test.Db.Reducers.InsertOneU32(3);
    test.Db.Reducers.InsertOneU64(4);
    test.Db.Reducers.InsertOneU128(u128);
    test.Db.Reducers.InsertOneU256(u256);
    test.Db.Reducers.InsertOneI8(-1);
    test.Db.Reducers.InsertOneI16(-2);
    test.Db.Reducers.InsertOneI32(-3);
    test.Db.Reducers.InsertOneI64(-4);
    test.Db.Reducers.InsertOneI128(i128);
    test.Db.Reducers.InsertOneI256(i256);
    test.Db.Reducers.InsertOneBool(true);
    test.Db.Reducers.InsertOneF32(1.25f);
    test.Db.Reducers.InsertOneF64(2.5);
    test.Db.Reducers.InsertOneString("hello");
    test.FrameTickUntil(() => remaining == 0);
}

void RunDeletePrimitive()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.UniqueU8()));
    var inserted = false;
    var deleted = false;

    test.Db.Db.UniqueU8.OnInsert += (_, row) =>
    {
        Require(row.N == 7 && row.Data == 10, "Unexpected unique_u8 insert row");
        inserted = true;
    };
    test.Db.Db.UniqueU8.OnDelete += (_, row) =>
    {
        Require(row.N == 7 && row.Data == 10, "Unexpected unique_u8 delete row");
        deleted = true;
    };

    test.Db.Reducers.InsertUniqueU8(7, 10);
    test.FrameTickUntil(() => inserted);
    test.Db.Reducers.DeleteUniqueU8(7);
    test.FrameTickUntil(() => deleted);
    Require(test.Db.Db.UniqueU8.Count == 0, $"Expected unique_u8 cache count 0, got {test.Db.Db.UniqueU8.Count}");
}

void RunUpdatePrimitive()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.PkU32()));
    ExpectPkU32Update(test, 9, 11, 12);
}

void RunInsertIdentity()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneIdentity()));
    var identity = Identity.FromHexString("0000000000000000000000000000000000000000000000000000000000000001");
    var inserted = false;

    test.Db.Db.OneIdentity.OnInsert += (_, row) =>
    {
        Require(row.I == identity, "Inserted identity did not round-trip");
        inserted = true;
    };

    test.Db.Reducers.InsertOneIdentity(identity);
    test.FrameTickUntil(() => inserted);
}

void RunInsertCallerIdentity()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneIdentity()));
    var inserted = false;

    test.Db.Db.OneIdentity.OnInsert += (_, row) =>
    {
        Require(row.I == test.Identity, "Inserted caller identity did not match connection identity");
        inserted = true;
    };

    test.Db.Reducers.InsertCallerOneIdentity();
    test.FrameTickUntil(() => inserted);
}

void RunDeleteIdentity()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.UniqueIdentity()));
    var inserted = false;
    var deleted = false;

    test.Db.Db.UniqueIdentity.OnInsert += (_, row) =>
    {
        Require(row.I == test.Identity && row.Data == 10, "Unexpected unique_identity insert row");
        inserted = true;
    };
    test.Db.Db.UniqueIdentity.OnDelete += (_, row) =>
    {
        Require(row.I == test.Identity && row.Data == 10, "Unexpected unique_identity delete row");
        deleted = true;
    };

    test.Db.Reducers.InsertUniqueIdentity(test.Identity, 10);
    test.FrameTickUntil(() => inserted);
    test.Db.Reducers.DeleteUniqueIdentity(test.Identity);
    test.FrameTickUntil(() => deleted);
}

void RunUpdateIdentity()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.PkIdentity()));
    var updated = false;
    test.Db.Db.PkIdentity.OnUpdate += (_, oldRow, newRow) =>
    {
        Require(oldRow.I == test.Identity && oldRow.Data == 10, "Unexpected old pk_identity row");
        Require(newRow.I == test.Identity && newRow.Data == 20, "Unexpected new pk_identity row");
        updated = true;
    };
    test.Db.Reducers.InsertPkIdentity(test.Identity, 10);
    test.FrameTickUntil(() => test.Db.Db.PkIdentity.Count == 1);
    test.Db.Reducers.UpdatePkIdentity(test.Identity, 20);
    test.FrameTickUntil(() => updated);
}

void RunInsertConnectionId()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneConnectionId()));
    var inserted = false;
    test.Db.Db.OneConnectionId.OnInsert += (_, row) =>
    {
        Require(row.A == test.Db.ConnectionId, "ConnectionId did not round-trip");
        inserted = true;
    };
    test.Db.Reducers.InsertOneConnectionId(test.Db.ConnectionId);
    test.FrameTickUntil(() => inserted);
}

void RunInsertCallerConnectionId()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneConnectionId()));
    var inserted = false;
    test.Db.Db.OneConnectionId.OnInsert += (_, row) =>
    {
        Require(row.A == test.Db.ConnectionId, "Caller ConnectionId did not match connection state");
        inserted = true;
    };
    test.Db.Reducers.InsertCallerOneConnectionId();
    test.FrameTickUntil(() => inserted);
}

void RunDeleteConnectionId()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.UniqueConnectionId()));
    var deleted = false;
    test.Db.Reducers.InsertUniqueConnectionId(test.Db.ConnectionId, 10);
    test.FrameTickUntil(() => test.Db.Db.UniqueConnectionId.Count == 1);
    test.Db.Db.UniqueConnectionId.OnDelete += (_, row) =>
    {
        Require(row.A == test.Db.ConnectionId && row.Data == 10, "Unexpected unique_connection_id delete row");
        deleted = true;
    };
    test.Db.Reducers.DeleteUniqueConnectionId(test.Db.ConnectionId);
    test.FrameTickUntil(() => deleted);
}

void RunUpdateConnectionId()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.PkConnectionId()));
    var updated = false;
    test.Db.Db.PkConnectionId.OnUpdate += (_, oldRow, newRow) =>
    {
        Require(oldRow.A == test.Db.ConnectionId && oldRow.Data == 10, "Unexpected old pk_connection_id row");
        Require(newRow.A == test.Db.ConnectionId && newRow.Data == 20, "Unexpected new pk_connection_id row");
        updated = true;
    };
    test.Db.Reducers.InsertPkConnectionId(test.Db.ConnectionId, 10);
    test.FrameTickUntil(() => test.Db.Db.PkConnectionId.Count == 1);
    test.Db.Reducers.UpdatePkConnectionId(test.Db.ConnectionId, 20);
    test.FrameTickUntil(() => updated);
}

void RunInsertTimestamp()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneTimestamp()));
    var timestamp = new Timestamp(1_234_567);
    var inserted = false;
    test.Db.Db.OneTimestamp.OnInsert += (_, row) =>
    {
        Require(row.T == timestamp, "Timestamp did not round-trip");
        inserted = true;
    };
    test.Db.Reducers.InsertOneTimestamp(timestamp);
    test.FrameTickUntil(() => inserted);
}

void RunInsertCallTimestamp()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneTimestamp()));
    var inserted = false;
    test.Db.Db.OneTimestamp.OnInsert += (ctx, row) =>
    {
        Require(ctx.Event is Event<Reducer>.Reducer, "Expected reducer event for insert_call_timestamp");
        Require(row.T.MicrosecondsSinceUnixEpoch > 0, "Reducer timestamp was not populated");
        inserted = true;
    };
    test.Db.Reducers.InsertCallTimestamp();
    test.FrameTickUntil(() => inserted);
}

void RunInsertUuid()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneUuid()));
    var uuid = Uuid.Parse("01890f3d-8120-7cc8-9a1f-cd1224fb3a10");
    var inserted = false;
    test.Db.Db.OneUuid.OnInsert += (_, row) =>
    {
        Require(row.U == uuid, "UUID did not round-trip");
        inserted = true;
    };
    test.Db.Reducers.InsertOneUuid(uuid);
    test.FrameTickUntil(() => inserted);
}

void RunInsertCallUuidV4() => RunGeneratedUuid(test => test.Db.Reducers.InsertCallUuidV4());

void RunInsertCallUuidV7() => RunGeneratedUuid(test => test.Db.Reducers.InsertCallUuidV7());

void RunGeneratedUuid(Action<HarnessConnection> callReducer)
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneUuid()));
    var inserted = false;
    test.Db.Db.OneUuid.OnInsert += (_, row) =>
    {
        Require(row.U != Uuid.NIL, "Generated UUID was nil");
        inserted = true;
    };
    callReducer(test);
    test.FrameTickUntil(() => inserted);
}

void RunDeleteUuid()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.UniqueUuid()));
    var uuid = Uuid.NIL;
    var deleted = false;
    test.Db.Reducers.InsertUniqueUuid(uuid, 10);
    test.FrameTickUntil(() => test.Db.Db.UniqueUuid.Count == 1);
    test.Db.Db.UniqueUuid.OnDelete += (_, row) =>
    {
        Require(row.U == uuid && row.Data == 10, "Unexpected unique_uuid delete row");
        deleted = true;
    };
    test.Db.Reducers.DeleteUniqueUuid(uuid);
    test.FrameTickUntil(() => deleted);
}

void RunUpdateUuid()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.PkUuid()));
    var uuid = Uuid.NIL;
    var updated = false;
    test.Db.Db.PkUuid.OnUpdate += (_, oldRow, newRow) =>
    {
        Require(oldRow.U == uuid && oldRow.Data == 10, "Unexpected old pk_uuid row");
        Require(newRow.U == uuid && newRow.Data == 20, "Unexpected new pk_uuid row");
        updated = true;
    };
    test.Db.Reducers.InsertPkUuid(uuid, 10);
    test.FrameTickUntil(() => test.Db.Db.PkUuid.Count == 1);
    test.Db.Reducers.UpdatePkUuid(uuid, 20);
    test.FrameTickUntil(() => updated);
}

void RunOnReducer()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneU8()));
    var callbackSeen = false;
    test.Db.Reducers.OnInsertOneU8 += (ctx, n) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(n == 128, "Unexpected reducer argument");
        Require(test.Db.Db.OneU8.Count == 1, "Reducer callback did not observe inserted row");
        callbackSeen = true;
    };
    test.Db.Reducers.InsertOneU8(128);
    test.FrameTickUntil(() => callbackSeen);
}

void RunFailReducer()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.PkU8()));
    var sawSuccess = false;
    var sawFailure = false;

    test.Db.Reducers.OnInsertPkU8 += (ctx, n, data) =>
    {
        if (ctx.Event.Status is Status.Committed)
        {
            Require(n == 1 && data == 10, "Unexpected successful insert_pk_u8 args");
            sawSuccess = true;
            test.Db.Reducers.InsertPkU8(1, 20);
        }
        else if (ctx.Event.Status is Status.Failed)
        {
            Require(n == 1 && data == 20, "Unexpected failed insert_pk_u8 args");
            sawFailure = true;
        }
    };

    test.Db.Reducers.InsertPkU8(1, 10);
    test.FrameTickUntil(() => sawSuccess && sawFailure);
    Require(test.Db.Db.PkU8.Count == 1, "Failed duplicate primary-key insert mutated the cache");
}

void RunInsertVec()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder()
        .AddQuery(qb => qb.From.VecI32())
        .AddQuery(qb => qb.From.VecString())
        .AddQuery(qb => qb.From.VecUuid()));
    var remaining = 3;
    var uuid = Uuid.Parse("01890f3d-8120-7cc8-9a1f-cd1224fb3a10");
    test.Db.Db.VecI32.OnInsert += (_, row) => { Require(row.N.SequenceEqual(new[] { -1, 0, 42 }), "VecI32 did not round-trip"); remaining--; };
    test.Db.Db.VecString.OnInsert += (_, row) => { Require(row.S.SequenceEqual(new[] { "alpha", "beta" }), "VecString did not round-trip"); remaining--; };
    test.Db.Db.VecUuid.OnInsert += (_, row) => { Require(row.U.SequenceEqual(new[] { uuid }), "VecUuid did not round-trip"); remaining--; };
    test.Db.Reducers.InsertVecI32(new() { -1, 0, 42 });
    test.Db.Reducers.InsertVecString(new() { "alpha", "beta" });
    test.Db.Reducers.InsertVecUuid(new() { uuid });
    test.FrameTickUntil(() => remaining == 0);
}

void RunInsertOptionSome()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder()
        .AddQuery(qb => qb.From.OptionI32())
        .AddQuery(qb => qb.From.OptionString())
        .AddQuery(qb => qb.From.OptionUuid()));
    var remaining = 3;
    var uuid = Uuid.Parse("01890f3d-8120-7cc8-9a1f-cd1224fb3a10");
    test.Db.Db.OptionI32.OnInsert += (_, row) => { Require(row.N == 42, "OptionI32 Some did not round-trip"); remaining--; };
    test.Db.Db.OptionString.OnInsert += (_, row) => { Require(row.S == "present", "OptionString Some did not round-trip"); remaining--; };
    test.Db.Db.OptionUuid.OnInsert += (_, row) => { Require(row.U == uuid, "OptionUuid Some did not round-trip"); remaining--; };
    test.Db.Reducers.InsertOptionI32(42);
    test.Db.Reducers.InsertOptionString("present");
    test.Db.Reducers.InsertOptionUuid(uuid);
    test.FrameTickUntil(() => remaining == 0);
}

void RunInsertOptionNone()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder()
        .AddQuery(qb => qb.From.OptionI32())
        .AddQuery(qb => qb.From.OptionString())
        .AddQuery(qb => qb.From.OptionUuid()));
    var remaining = 3;
    test.Db.Db.OptionI32.OnInsert += (_, row) => { Require(row.N == null, "OptionI32 None did not round-trip"); remaining--; };
    test.Db.Db.OptionString.OnInsert += (_, row) => { Require(row.S == null, "OptionString None did not round-trip"); remaining--; };
    test.Db.Db.OptionUuid.OnInsert += (_, row) => { Require(row.U == null, "OptionUuid None did not round-trip"); remaining--; };
    test.Db.Reducers.InsertOptionI32(null);
    test.Db.Reducers.InsertOptionString(null);
    test.Db.Reducers.InsertOptionUuid(null);
    test.FrameTickUntil(() => remaining == 0);
}

void RunInsertStruct()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder()
        .AddQuery(qb => qb.From.OneByteStruct())
        .AddQuery(qb => qb.From.OneEveryPrimitiveStruct()));
    var remaining = 2;
    var primitive = EveryPrimitiveStructValue(test);
    test.Db.Db.OneByteStruct.OnInsert += (_, row) => { Require(row.S.B == 99, "ByteStruct did not round-trip"); remaining--; };
    test.Db.Db.OneEveryPrimitiveStruct.OnInsert += (_, row) => { Require(row.S == primitive, "EveryPrimitiveStruct did not round-trip"); remaining--; };
    test.Db.Reducers.InsertOneByteStruct(new ByteStruct { B = 99 });
    test.Db.Reducers.InsertOneEveryPrimitiveStruct(primitive);
    test.FrameTickUntil(() => remaining == 0);
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

void RunInsertDeleteLargeTable()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.LargeTable()));
    var large = LargeTableValue(test);
    var inserted = false;
    var deleted = false;
    test.Db.Db.LargeTable.OnInsert += (_, row) =>
    {
        Require(row == large, "LargeTable insert did not round-trip");
        inserted = true;
        CallDeleteLargeTable(test, large);
    };
    test.Db.Db.LargeTable.OnDelete += (_, row) =>
    {
        Require(row == large, "LargeTable delete did not round-trip");
        deleted = true;
    };
    CallInsertLargeTable(test, large);
    test.FrameTickUntil(() => inserted && deleted);
}

void RunInsertPrimitivesAsStrings()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.VecString()));
    var primitive = EveryPrimitiveStructValue(test);
    var expected = new[]
    {
        primitive.A.ToString(),
        primitive.B.ToString(),
        primitive.C.ToString(),
        primitive.D.ToString(),
        primitive.E.ToString(),
        primitive.F.ToString(),
        primitive.G.ToString(),
        primitive.H.ToString(),
        primitive.I.ToString(),
        primitive.J.ToString(),
        primitive.K.ToString(),
        primitive.L.ToString(),
        primitive.M.ToString().ToLowerInvariant(),
        primitive.N.ToString(),
        primitive.O.ToString(),
        primitive.P,
        primitive.Q.ToString(),
        primitive.R.ToString(),
        primitive.S.ToString(),
        primitive.T.ToString(),
        primitive.U.ToString(),
    };
    var inserted = false;
    test.Db.Db.VecString.OnInsert += (_, row) =>
    {
        Require(row.S.SequenceEqual(expected), "Primitive string conversion did not round-trip");
        inserted = true;
    };
    test.Db.Reducers.InsertPrimitivesAsStrings(primitive);
    test.FrameTickUntil(() => inserted);
}

void RunReauth()
{
    var tokenPath = Path.Combine(Path.GetTempPath(), $"spacetimedb-csharp-sdk-test-{dbName}.token");
    string? token = null;
    using (var first = Connect(onConnect: (_, _, receivedToken) =>
    {
        token = receivedToken;
        File.WriteAllText(tokenPath, receivedToken);
    }))
    {
        first.FrameTickUntil(() => token != null);
    }

    token = File.ReadAllText(tokenPath);
    using var second = Connect(token: token, onConnect: (_, identity, receivedToken) =>
    {
        Require(receivedToken == token, "Reauth connection returned a different token");
        Require(identity != default, "Reauth connection returned default identity");
    });
    second.FrameTickUntil(() => second.Db.Identity != null);
}

void RunReconnectDifferentConnectionId()
{
    ConnectionId? firstConnectionId = null;
    using (var first = Connect(allowCleanDisconnect: true))
    {
        firstConnectionId = first.Db.ConnectionId;
    }

    using var second = Connect();
    second.FrameTickUntil(() => second.Db.Identity != null);
    Require(second.Db.ConnectionId != firstConnectionId, "Reconnect reused the prior connection id");
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

void RunCallerAliceReceivesReducerCallbackButNotBob()
{
    using var alice = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneU8()).AddQuery(qb => qb.From.OneU16()));
    using var bob = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneU8()).AddQuery(qb => qb.From.OneU16()));
    Require(alice.Identity != bob.Identity, "Alice and Bob should have distinct identities");

    var aliceRows = 0;
    var bobRows = 0;
    var aliceReducerCallback = false;
    var bobReducerCallback = false;
    alice.Db.Db.OneU8.OnInsert += (_, row) => { Require(row.N == 42, "Alice saw wrong one_u8 value"); aliceRows++; };
    bob.Db.Db.OneU8.OnInsert += (_, row) => { Require(row.N == 42, "Bob saw wrong one_u8 value"); bobRows++; };
    alice.Db.Db.OneU16.OnInsert += (_, row) => { Require(row.N == 24, "Alice saw wrong one_u16 value"); aliceRows++; };
    bob.Db.Db.OneU16.OnInsert += (_, row) => { Require(row.N == 24, "Bob saw wrong one_u16 value"); bobRows++; };
    alice.Db.Reducers.OnInsertOneU8 += (ctx, n) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(n == 42, "Alice reducer callback saw wrong argument");
        aliceReducerCallback = true;
    };
    bob.Db.Reducers.OnInsertOneU8 += (_, _) => bobReducerCallback = true;

    alice.Db.Reducers.InsertOneU8(42);
    alice.Db.Reducers.InsertOneU16(24);
    FrameTickUntil(new[] { alice, bob }, () => aliceRows == 2 && bobRows == 2 && aliceReducerCallback);
    Require(!bobReducerCallback, "Bob received Alice's reducer callback");
}

void RunRowDeduplication()
{
    using var test = ConnectAndSubscribeSql(
        "SELECT * FROM pk_u32 WHERE n < 100",
        "SELECT * FROM pk_u32 WHERE n < 200");
    var ins24 = Once("insert 24");
    var ins42 = Once("insert 42");
    var del24 = Once("delete 24");
    var upd42 = Once("update 42");

    test.Db.Db.PkU32.OnInsert += (_, row) =>
    {
        if (row.N == 24)
        {
            ins24.Invoke();
            test.Db.Reducers.DeletePkU32(24);
        }
        else if (row.N == 42)
        {
            ins42.Invoke();
            test.Db.Reducers.UpdatePkU32(42, 0xfeeb);
        }
        else
        {
            throw new Exception($"Unexpected pk_u32 insert {row.N}");
        }
    };
    test.Db.Db.PkU32.OnDelete += (_, row) =>
    {
        Require(row.N == 24, "Only row 24 should be deleted");
        del24.Invoke();
    };
    test.Db.Db.PkU32.OnUpdate += (_, oldRow, newRow) =>
    {
        Require(oldRow.N == 42 && oldRow.Data == 0xbeef && newRow.N == 42 && newRow.Data == 0xfeeb, "Unexpected pk_u32 update");
        upd42.Invoke();
    };

    test.Db.Reducers.InsertPkU32(24, 0xbeef);
    test.Db.Reducers.InsertPkU32(42, 0xbeef);
    test.FrameTickUntil(() => ins24.Done && ins42.Done && del24.Done && upd42.Done);
    Require(test.Db.Db.PkU32.Count == 1, "Deduplicated cache should contain one row");
}

void RunRowDeduplicationJoinRAndS()
{
    using var test = ConnectAndSubscribeSql(
        "SELECT * FROM pk_u32",
        "SELECT unique_u32.* FROM unique_u32 JOIN pk_u32 ON unique_u32.n = pk_u32.n");
    var pkInsert = false;
    var pkUpdate = false;
    var uniqueInsert = false;
    test.Db.Db.PkU32.OnInsert += (_, row) =>
    {
        Require(row.N == 42 && row.Data == 50, "Unexpected pk_u32 insert");
        pkInsert = true;
        test.Db.Reducers.InsertUniqueU32UpdatePkU32(42, 0xbeef, 100);
    };
    test.Db.Db.PkU32.OnUpdate += (_, oldRow, newRow) =>
    {
        Require(oldRow.N == 42 && oldRow.Data == 50 && newRow.N == 42 && newRow.Data == 100, "Unexpected pk_u32 update");
        pkUpdate = true;
    };
    test.Db.Db.UniqueU32.OnInsert += (_, row) =>
    {
        Require(row.N == 42 && row.Data == 0xbeef, "Unexpected unique_u32 insert");
        uniqueInsert = true;
    };
    test.Db.Db.UniqueU32.OnDelete += (_, _) => throw new Exception("unique_u32 should not be deleted");
    test.Db.Reducers.InsertPkU32(42, 50);
    test.FrameTickUntil(() => pkInsert && pkUpdate && uniqueInsert);
}

void RunRowDeduplicationRJoinSAndRJoinT()
{
    using var test = ConnectAndSubscribeSql(
        "SELECT * FROM pk_u32",
        "SELECT * FROM pk_u32_two",
        "SELECT unique_u32.* FROM unique_u32 JOIN pk_u32 ON unique_u32.n = pk_u32.n",
        "SELECT unique_u32.* FROM unique_u32 JOIN pk_u32_two ON unique_u32.n = pk_u32_two.n");
    var pkInsert = false;
    var pkDelete = false;
    var pkTwoInsert = false;
    var uniqueInserts = 0;
    test.Db.Reducers.InsertUniqueU32(42, 0xbeef);
    test.FrameTickUntil(() => true);
    test.Db.Db.PkU32.OnInsert += (_, row) =>
    {
        Require(row.N == 42 && row.Data == 0xbeef, "Unexpected pk_u32 insert");
        pkInsert = true;
        test.Db.Reducers.DeletePkU32InsertPkU32Two(42, 0xbeef);
    };
    test.Db.Db.PkU32.OnDelete += (_, row) =>
    {
        Require(row.N == 42 && row.Data == 0xbeef, "Unexpected pk_u32 delete");
        pkDelete = true;
    };
    test.Db.Db.PkU32Two.OnInsert += (_, row) =>
    {
        Require(row.N == 42 && row.Data == 0xbeef, "Unexpected pk_u32_two insert");
        pkTwoInsert = true;
    };
    test.Db.Db.UniqueU32.OnInsert += (_, _) => uniqueInserts++;
    test.Db.Reducers.InsertPkU32(42, 0xbeef);
    test.FrameTickUntil(() => pkInsert && pkDelete && pkTwoInsert);
    Require(uniqueInserts <= 1, $"Expected at most one deduplicated unique_u32 insert, got {uniqueInserts}");
}

void RunLhsJoinUpdate(bool disjoint)
{
    var queries = disjoint
        ? new[]
        {
            "SELECT p.* FROM pk_u32 p WHERE n = 1",
            "SELECT p.* FROM pk_u32 p JOIN unique_u32 u ON p.n = u.n WHERE u.data > 0 AND u.data < 5 AND u.n != 1",
        }
        : new[]
        {
            "SELECT p.* FROM pk_u32 p WHERE n = 1",
            "SELECT p.* FROM pk_u32 p JOIN unique_u32 u ON p.n = u.n WHERE u.data > 0 AND u.data < 5",
        };
    using var test = ConnectAndSubscribeSql(queries);
    var insertedRows = 0;
    var update1 = false;
    var update2 = false;
    test.Db.Db.PkU32.OnInsert += (_, row) =>
    {
        if (row.N is 1 or 2)
        {
            insertedRows++;
        }
    };
    test.Db.Db.PkU32.OnUpdate += (_, oldRow, newRow) =>
    {
        if (oldRow.N == 2 && oldRow.Data == 0 && newRow.N == 2 && newRow.Data == 1)
        {
            update1 = true;
            test.Db.Reducers.UpdatePkU32(2, 0);
        }
        else if (oldRow.N == 2 && oldRow.Data == 1 && newRow.N == 2 && newRow.Data == 0)
        {
            update2 = true;
        }
    };
    test.Db.Reducers.InsertPkU32(1, 0);
    test.Db.Reducers.InsertPkU32(2, 0);
    test.Db.Reducers.InsertUniqueU32(1, 3);
    test.Db.Reducers.InsertUniqueU32(2, 4);
    test.FrameTickUntil(() => insertedRows == 2);
    test.Db.Reducers.UpdatePkU32(2, 1);
    test.FrameTickUntil(() => update1 && update2);
}

void RunIntraQueryBagSemanticsForJoin()
{
    using var test = ConnectAndSubscribeSql(
        "SELECT * FROM btree_u32",
        "SELECT pk_u32.* FROM pk_u32 JOIN btree_u32 ON pk_u32.n = btree_u32.n");
    var pkInserts = 0;
    var pkDeletes = 0;
    var firstBtreeDeleteReducerSeen = false;
    var secondBtreeDeleteReducerSeen = false;

    test.Db.Db.PkU32.OnInsert += (_, row) =>
    {
        Require(row.N == 0 && row.Data == 0, "Unexpected pk_u32 insert");
        pkInserts++;
    };
    test.Db.Db.PkU32.OnDelete += (_, row) =>
    {
        Require(row.N == 0 && row.Data == 0, "Unexpected pk_u32 delete");
        pkDeletes++;
    };
    test.Db.Reducers.OnDeleteFromBtreeU32 += (ctx, rows) =>
    {
        RequireCommitted(ctx.Event.Status);
        var deleted = rows.Single();
        if (deleted.Data == 0)
        {
            Require(pkDeletes == 0, "pk_u32 was deleted while join multiplicity was still positive");
            firstBtreeDeleteReducerSeen = true;
        }
        else if (deleted.Data == 1)
        {
            secondBtreeDeleteReducerSeen = true;
        }
        else
        {
            throw new Exception($"Unexpected btree_u32 delete data {deleted.Data}");
        }
    };

    test.Db.Reducers.InsertIntoBtreeU32(new() { new BTreeU32(0, 0) });
    test.Db.Reducers.InsertIntoPkBtreeU32(new() { new PkU32(0, 0) }, new() { new BTreeU32(0, 1) });
    test.Db.Reducers.DeleteFromBtreeU32(new() { new BTreeU32(0, 0) });
    test.Db.Reducers.DeleteFromBtreeU32(new() { new BTreeU32(0, 1) });

    test.FrameTickUntil(() => firstBtreeDeleteReducerSeen && secondBtreeDeleteReducerSeen && pkDeletes == 1);
    Require(pkInserts == 1, $"Expected one pk_u32 insert, got {pkInserts}");
}

void RunTwoDifferentCompressionAlgos()
{
    var bytes = Enumerable.Range(0, 1 << 15).Select(i => (byte)(i % 251)).ToList();
    using var brotli = ConnectAndSubscribeCompression(Compression.Brotli, bytes);
    using var gzip = ConnectAndSubscribeCompression(Compression.Gzip, bytes);
    using var none = ConnectAndSubscribeCompression(Compression.None, bytes);
    none.Db.Reducers.InsertVecU8(bytes);
    FrameTickUntil(new[] { brotli, gzip, none }, () =>
        brotli.Db.Db.VecU8.Count == 1 && gzip.Db.Db.VecU8.Count == 1 && none.Db.Db.VecU8.Count == 1);
}

void RunParameterizedSubscription()
{
    using var client0 = ConnectAndSubscribeSql("SELECT * FROM pk_identity WHERE i = :sender");
    using var client1 = ConnectAndSubscribeSql("SELECT * FROM pk_identity WHERE i = :sender");
    var insert0 = false;
    var update0 = false;
    var insert1 = false;
    var update1 = false;
    client0.Db.Db.PkIdentity.OnInsert += (_, row) => { Require(row.I == client0.Identity && row.Data == 1, "client0 insert mismatch"); insert0 = true; };
    client0.Db.Db.PkIdentity.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.I == client0.Identity && newRow.I == client0.Identity && oldRow.Data == 1 && newRow.Data == 2, "client0 update mismatch"); update0 = true; };
    client1.Db.Db.PkIdentity.OnInsert += (_, row) => { Require(row.I == client1.Identity && row.Data == 3, "client1 insert mismatch"); insert1 = true; };
    client1.Db.Db.PkIdentity.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.I == client1.Identity && newRow.I == client1.Identity && oldRow.Data == 3 && newRow.Data == 4, "client1 update mismatch"); update1 = true; };
    client0.Db.Reducers.InsertPkIdentity(client0.Identity, 1);
    client0.Db.Reducers.UpdatePkIdentity(client0.Identity, 2);
    client1.Db.Reducers.InsertPkIdentity(client1.Identity, 3);
    client1.Db.Reducers.UpdatePkIdentity(client1.Identity, 4);
    FrameTickUntil(new[] { client0, client1 }, () => insert0 && update0 && insert1 && update1);
}

void RunRlsSubscription()
{
    using var alice = ConnectAndSubscribeSql("SELECT * FROM users");
    using var bob = ConnectAndSubscribeSql("SELECT * FROM users");
    var aliceInserted = false;
    var bobInserted = false;
    alice.Db.Db.Users.OnInsert += (_, row) =>
    {
        Require(row.Name == "Alice" && row.Identity == alice.Identity, "Alice saw wrong RLS row");
        aliceInserted = true;
    };
    bob.Db.Db.Users.OnInsert += (_, row) =>
    {
        Require(row.Name == "Bob" && row.Identity == bob.Identity, "Bob saw wrong RLS row");
        bobInserted = true;
    };
    alice.Db.Reducers.InsertUser("Alice", alice.Identity);
    bob.Db.Reducers.InsertUser("Bob", bob.Identity);
    FrameTickUntil(new[] { alice, bob }, () => aliceInserted && bobInserted);
}

void RunPkSimpleEnum()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.PkSimpleEnum()));
    var updated = false;
    var enumValue = SimpleEnum.Two;
    test.Db.Db.PkSimpleEnum.OnInsert += (_, row) =>
    {
        Require(row.A == enumValue && row.Data == 42, "Unexpected pk_simple_enum insert");
        test.Db.Reducers.UpdatePkSimpleEnum(enumValue, 24);
    };
    test.Db.Db.PkSimpleEnum.OnUpdate += (_, oldRow, newRow) =>
    {
        Require(oldRow.A == enumValue && oldRow.Data == 42, "Unexpected old pk_simple_enum row");
        Require(newRow.A == enumValue && newRow.Data == 24, "Unexpected new pk_simple_enum row");
        updated = true;
    };
    test.Db.Db.PkSimpleEnum.OnDelete += (_, _) => throw new Exception("pk_simple_enum should not be deleted");
    test.Db.Reducers.InsertPkSimpleEnum(enumValue, 42);
    test.FrameTickUntil(() => updated);
}

void RunIndexedSimpleEnum()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.IndexedSimpleEnum()));
    var updated = false;
    test.Db.Db.IndexedSimpleEnum.OnInsert += (_, row) =>
    {
        if (row.N == SimpleEnum.Two)
        {
            test.Db.Reducers.UpdateIndexedSimpleEnum(SimpleEnum.Two, SimpleEnum.One);
        }
        else if (row.N == SimpleEnum.One)
        {
            updated = true;
        }
    };
    test.Db.Reducers.InsertIntoIndexedSimpleEnum(SimpleEnum.Two);
    test.FrameTickUntil(() => updated);
}

void RunOverlappingSubscriptions()
{
    using var test = Connect();
    test.Db.Reducers.InsertPkU8(1, 0);
    test.FrameTickUntil(() => true);
    var applied = false;
    test.Db.SubscriptionBuilder()
        .OnApplied(ctx =>
        {
            Require(ctx.Db.PkU8.Count == 1, "Overlapping initial subscription should deduplicate matching row");
            applied = true;
        })
        .OnError((_, err) => throw err)
        .Subscribe(new[] { "SELECT * FROM pk_u8 WHERE n < 100", "SELECT * FROM pk_u8 WHERE n > 0" });
    test.FrameTickUntil(() => applied);
    var updated = false;
    test.Db.Db.PkU8.OnUpdate += (_, oldRow, newRow) =>
    {
        Require(oldRow.N == 1 && oldRow.Data == 0 && newRow.N == 1 && newRow.Data == 1, "Overlapping update was wrong");
        updated = true;
    };
    test.Db.Reducers.UpdatePkU8(1, 1);
    test.FrameTickUntil(() => updated);
}

void RunSortedUuidsInsert()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.PkUuid()));
    var rows = 0;
    var reducerSeen = false;
    test.Db.Db.PkUuid.OnInsert += (_, _) => rows++;
    test.Db.Reducers.OnSortedUuidsInsert += ctx =>
    {
        RequireCommitted(ctx.Event.Status);
        reducerSeen = true;
    };
    test.Db.Reducers.SortedUuidsInsert();
    test.FrameTickUntil(() => reducerSeen && rows == 1000, timeoutSeconds: 30);
    Require(test.Db.Db.PkUuid.Count == 1000, "Expected 1000 UUID rows");
}

void ExpectPkU32Update(HarnessConnection test, uint key, int initialData, int updatedData)
{
    var inserted = false;
    var updated = false;
    test.Db.Db.PkU32.OnInsert += (_, row) =>
    {
        if (row.N == key && row.Data == initialData)
        {
            inserted = true;
        }
    };
    test.Db.Db.PkU32.OnUpdate += (_, oldRow, newRow) =>
    {
        Require(oldRow.N == key && oldRow.Data == initialData, "Unexpected old pk_u32 row");
        Require(newRow.N == key && newRow.Data == updatedData, "Unexpected new pk_u32 row");
        updated = true;
    };
    test.Db.Reducers.InsertPkU32(key, initialData);
    test.FrameTickUntil(() => inserted);
    test.Db.Reducers.UpdatePkU32(key, updatedData);
    test.FrameTickUntil(() => updated);
}

HarnessConnection ConnectAndSubscribe(Func<DbConnection, TypedSubscriptionBuilder> buildSubscription, Compression? compression = null)
{
    DbConnection db = null!;
    var connected = false;
    var applied = false;

    db = BuildConnection(compression: compression)
        .OnConnect((conn, identity, _) =>
        {
            Require(identity == conn.Identity, "Connection identity callback did not match connection state");
            buildSubscription(conn)
                .OnApplied(_ => applied = true)
                .OnError((_, err) => throw err)
                .Subscribe();
            connected = true;
        })
        .Build();

    var harness = new HarnessConnection(db);
    harness.FrameTickUntil(() => connected && applied);
    return harness;
}

HarnessConnection ConnectAndSubscribeSql(params string[] queries)
{
    var connected = false;
    var applied = false;
    DbConnection db = null!;
    db = BuildConnection()
        .OnConnect((conn, _, _) =>
        {
            conn.SubscriptionBuilder()
                .OnApplied(_ => applied = true)
                .OnError((_, err) => throw err)
                .Subscribe(queries);
            connected = true;
        })
        .Build();
    var harness = new HarnessConnection(db);
    harness.FrameTickUntil(() => connected && applied);
    return harness;
}

HarnessConnection ConnectAndSubscribeAll()
{
    var connected = false;
    var applied = false;
    DbConnection db = null!;
    db = BuildConnection()
        .OnConnect((conn, _, _) =>
        {
            conn.SubscriptionBuilder()
                .OnApplied(_ => applied = true)
                .OnError((_, err) => throw err)
                .SubscribeToAllTables();
            connected = true;
        })
        .Build();
    var harness = new HarnessConnection(db);
    harness.FrameTickUntil(() => connected && applied);
    return harness;
}

HarnessConnection ConnectAndSubscribeCompression(Compression compression, List<byte> expected)
{
    var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.VecU8()), compression);
    test.Db.Db.VecU8.OnInsert += (_, row) =>
    {
        Require(row.N.SequenceEqual(expected), $"{compression} subscription received wrong bytes");
    };
    return test;
}

HarnessConnection Connect(
    string? token = null,
    bool allowCleanDisconnect = false,
    Action<DbConnection, Identity, string>? onConnect = null,
    Compression? compression = null)
{
    var connected = false;
    var builder = BuildConnection(token, allowCleanDisconnect, compression)
        .OnConnect((conn, identity, receivedToken) =>
        {
            Require(identity == conn.Identity, "Connection identity callback did not match connection state");
            connected = true;
            onConnect?.Invoke(conn, identity, receivedToken);
        });
    var harness = new HarnessConnection(builder.Build(), allowCleanDisconnect);
    harness.FrameTickUntil(() => connected);
    return harness;
}

DbConnectionBuilder<DbConnection> BuildConnection(string? token = null, bool allowCleanDisconnect = false, Compression? compression = null)
{
    var builder = DbConnection
        .Builder()
        .WithUri(serverUrl)
        .WithDatabaseName(dbName)
        .WithToken(token)
        .OnConnectError(err => throw err)
        .OnDisconnect((_, err) =>
        {
            if (allowCleanDisconnect && err == null)
            {
                return;
            }
            throw err ?? new Exception("Connection disconnected unexpectedly");
        });
    if (compression != null)
    {
        builder.WithCompression(compression.Value);
    }
    return builder;
}

EveryPrimitiveStruct EveryPrimitiveStructValue(HarnessConnection test) => new()
{
    A = 1,
    B = 2,
    C = 3,
    D = 4,
    E = new U128(0, 5),
    F = new U256(new U128(0, 0), new U128(0, 6)),
    G = -1,
    H = -2,
    I = -3,
    J = -4,
    K = new I128(0, 5),
    L = new I256(new U128(0, 0), new U128(0, 6)),
    M = true,
    N = 1.25f,
    O = 2.5,
    P = "primitive",
    Q = test.Identity,
    R = test.Db.ConnectionId,
    S = new Timestamp(1_234_567),
    T = new TimeDuration(9_876),
    U = Uuid.Parse("01890f3d-8120-7cc8-9a1f-cd1224fb3a10"),
};

EveryVecStruct EveryVecStructValue(HarnessConnection test) => new()
{
    A = new() { 1 },
    B = new() { 2 },
    C = new() { 3 },
    D = new() { 4 },
    E = new() { new U128(0, 5) },
    F = new() { new U256(new U128(0, 0), new U128(0, 6)) },
    G = new() { -1 },
    H = new() { -2 },
    I = new() { -3 },
    J = new() { -4 },
    K = new() { new I128(0, 5) },
    L = new() { new I256(new U128(0, 0), new U128(0, 6)) },
    M = new() { true },
    N = new() { 1.25f },
    O = new() { 2.5 },
    P = new() { "vec" },
    Q = new() { test.Identity },
    R = new() { test.Db.ConnectionId },
    S = new() { new Timestamp(1_234_567) },
    T = new() { new TimeDuration(9_876) },
    U = new() { Uuid.Parse("01890f3d-8120-7cc8-9a1f-cd1224fb3a10") },
};

LargeTable LargeTableValue(HarnessConnection test) => new()
{
    A = 1,
    B = 2,
    C = 3,
    D = 4,
    E = new U128(0, 5),
    F = new U256(new U128(0, 0), new U128(0, 6)),
    G = -1,
    H = -2,
    I = -3,
    J = -4,
    K = new I128(0, 5),
    L = new I256(new U128(0, 0), new U128(0, 6)),
    M = true,
    N = 1.25f,
    O = 2.5,
    P = "large",
    Q = SimpleEnum.Two,
    R = new EnumWithPayload.Str("payload"),
    S = new UnitStruct(),
    T = new ByteStruct { B = 9 },
    U = EveryPrimitiveStructValue(test),
    V = EveryVecStructValue(test),
};

void CallInsertLargeTable(HarnessConnection test, LargeTable row) =>
    test.Db.Reducers.InsertLargeTable(row.A, row.B, row.C, row.D, row.E, row.F, row.G, row.H, row.I, row.J, row.K, row.L, row.M, row.N, row.O, row.P, row.Q, row.R, row.S, row.T, row.U, row.V);

void CallDeleteLargeTable(HarnessConnection test, LargeTable row) =>
    test.Db.Reducers.DeleteLargeTable(row.A, row.B, row.C, row.D, row.E, row.F, row.G, row.H, row.I, row.J, row.K, row.L, row.M, row.N, row.O, row.P, row.Q, row.R, row.S, row.T, row.U, row.V);

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

OnceFlag Once(string name)
{
    var seen = false;
    return new OnceFlag(() =>
    {
        if (seen)
        {
            throw new Exception($"{name} callback fired more than once");
        }
        seen = true;
    }, () => seen);
}

void FrameTickUntil(IEnumerable<HarnessConnection> connections, Func<bool> isComplete, int timeoutSeconds = 20)
{
    var list = connections.ToArray();
    var deadline = DateTime.UtcNow.AddSeconds(timeoutSeconds);
    while (!isComplete())
    {
        foreach (var connection in list)
        {
            connection.Db.FrameTick();
        }
        Thread.Sleep(25);
        if (DateTime.UtcNow > deadline)
        {
            throw new TimeoutException($"Timed out after {timeoutSeconds} seconds");
        }
    }
}

sealed class OnceFlag
{
    private readonly Action mark;
    private readonly Func<bool> done;

    public OnceFlag(Action mark, Func<bool> done)
    {
        this.mark = mark;
        this.done = done;
    }

    public bool Done => done();

    public void Invoke() => mark();

    public static implicit operator Action(OnceFlag flag) => flag.Invoke;
}

sealed class HarnessConnection : IDisposable
{
    private readonly bool allowCleanDisconnect;

    public DbConnection Db { get; }

    public Identity Identity => Db.Identity ?? throw new InvalidOperationException("Connection has no identity yet");

    public HarnessConnection(DbConnection db, bool allowCleanDisconnect = false)
    {
        Db = db;
        this.allowCleanDisconnect = allowCleanDisconnect;
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
        try
        {
            Db.Disconnect();
        }
        catch when (allowCleanDisconnect)
        {
        }
    }
}
