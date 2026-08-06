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
    using var test = ConnectAndSubscribeAll();
    var u128 = new U128(0, 5);
    var u256 = new U256(new U128(0, 0), new U128(0, 6));
    var i128 = new I128(0, 7);
    var i256 = new I256(new U128(0, 0), new U128(0, 8));

    var remaining = 16;
    void Seen() => remaining--;

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
    using var test = ConnectAndSubscribeAll();
    var u128 = new U128(0, 5);
    var u256 = new U256(new U128(0, 0), new U128(0, 6));
    var i128 = new I128(0, 7);
    var i256 = new I256(new U128(0, 0), new U128(0, 8));
    var remaining = 14;
    void Seen() => remaining--;

    test.Db.Db.UniqueU8.OnDelete += (_, row) => { Require(row.N == 1 && row.Data == 10, "Unexpected unique_u8 delete row"); Seen(); };
    test.Db.Db.UniqueU16.OnDelete += (_, row) => { Require(row.N == 2 && row.Data == 10, "Unexpected unique_u16 delete row"); Seen(); };
    test.Db.Db.UniqueU32.OnDelete += (_, row) => { Require(row.N == 3 && row.Data == 10, "Unexpected unique_u32 delete row"); Seen(); };
    test.Db.Db.UniqueU64.OnDelete += (_, row) => { Require(row.N == 4 && row.Data == 10, "Unexpected unique_u64 delete row"); Seen(); };
    test.Db.Db.UniqueU128.OnDelete += (_, row) => { Require(row.N.Equals(u128) && row.Data == 10, "Unexpected unique_u128 delete row"); Seen(); };
    test.Db.Db.UniqueU256.OnDelete += (_, row) => { Require(row.N.Equals(u256) && row.Data == 10, "Unexpected unique_u256 delete row"); Seen(); };
    test.Db.Db.UniqueI8.OnDelete += (_, row) => { Require(row.N == -1 && row.Data == 10, "Unexpected unique_i8 delete row"); Seen(); };
    test.Db.Db.UniqueI16.OnDelete += (_, row) => { Require(row.N == -2 && row.Data == 10, "Unexpected unique_i16 delete row"); Seen(); };
    test.Db.Db.UniqueI32.OnDelete += (_, row) => { Require(row.N == -3 && row.Data == 10, "Unexpected unique_i32 delete row"); Seen(); };
    test.Db.Db.UniqueI64.OnDelete += (_, row) => { Require(row.N == -4 && row.Data == 10, "Unexpected unique_i64 delete row"); Seen(); };
    test.Db.Db.UniqueI128.OnDelete += (_, row) => { Require(row.N.Equals(i128) && row.Data == 10, "Unexpected unique_i128 delete row"); Seen(); };
    test.Db.Db.UniqueI256.OnDelete += (_, row) => { Require(row.N.Equals(i256) && row.Data == 10, "Unexpected unique_i256 delete row"); Seen(); };
    test.Db.Db.UniqueBool.OnDelete += (_, row) => { Require(row.B && row.Data == 10, "Unexpected unique_bool delete row"); Seen(); };
    test.Db.Db.UniqueString.OnDelete += (_, row) => { Require(row.S == "key" && row.Data == 10, "Unexpected unique_string delete row"); Seen(); };

    test.Db.Reducers.InsertUniqueU8(1, 10);
    test.Db.Reducers.InsertUniqueU16(2, 10);
    test.Db.Reducers.InsertUniqueU32(3, 10);
    test.Db.Reducers.InsertUniqueU64(4, 10);
    test.Db.Reducers.InsertUniqueU128(u128, 10);
    test.Db.Reducers.InsertUniqueU256(u256, 10);
    test.Db.Reducers.InsertUniqueI8(-1, 10);
    test.Db.Reducers.InsertUniqueI16(-2, 10);
    test.Db.Reducers.InsertUniqueI32(-3, 10);
    test.Db.Reducers.InsertUniqueI64(-4, 10);
    test.Db.Reducers.InsertUniqueI128(i128, 10);
    test.Db.Reducers.InsertUniqueI256(i256, 10);
    test.Db.Reducers.InsertUniqueBool(true, 10);
    test.Db.Reducers.InsertUniqueString("key", 10);
    test.FrameTickUntil(() => test.Db.Db.UniqueString.Count == 1);

    test.Db.Reducers.DeleteUniqueU8(1);
    test.Db.Reducers.DeleteUniqueU16(2);
    test.Db.Reducers.DeleteUniqueU32(3);
    test.Db.Reducers.DeleteUniqueU64(4);
    test.Db.Reducers.DeleteUniqueU128(u128);
    test.Db.Reducers.DeleteUniqueU256(u256);
    test.Db.Reducers.DeleteUniqueI8(-1);
    test.Db.Reducers.DeleteUniqueI16(-2);
    test.Db.Reducers.DeleteUniqueI32(-3);
    test.Db.Reducers.DeleteUniqueI64(-4);
    test.Db.Reducers.DeleteUniqueI128(i128);
    test.Db.Reducers.DeleteUniqueI256(i256);
    test.Db.Reducers.DeleteUniqueBool(true);
    test.Db.Reducers.DeleteUniqueString("key");
    test.FrameTickUntil(() => remaining == 0);
}

void RunUpdatePrimitive()
{
    using var test = ConnectAndSubscribeAll();
    var u128 = new U128(0, 5);
    var u256 = new U256(new U128(0, 0), new U128(0, 6));
    var i128 = new I128(0, 7);
    var i256 = new I256(new U128(0, 0), new U128(0, 8));
    var remaining = 14;
    void Seen() => remaining--;

    test.Db.Db.PkU8.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.N == 1 && oldRow.Data == 10 && newRow.N == 1 && newRow.Data == 20, "Unexpected pk_u8 update"); Seen(); };
    test.Db.Db.PkU16.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.N == 2 && oldRow.Data == 10 && newRow.N == 2 && newRow.Data == 20, "Unexpected pk_u16 update"); Seen(); };
    test.Db.Db.PkU32.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.N == 3 && oldRow.Data == 10 && newRow.N == 3 && newRow.Data == 20, "Unexpected pk_u32 update"); Seen(); };
    test.Db.Db.PkU64.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.N == 4 && oldRow.Data == 10 && newRow.N == 4 && newRow.Data == 20, "Unexpected pk_u64 update"); Seen(); };
    test.Db.Db.PkU128.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.N.Equals(u128) && oldRow.Data == 10 && newRow.N.Equals(u128) && newRow.Data == 20, "Unexpected pk_u128 update"); Seen(); };
    test.Db.Db.PkU256.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.N.Equals(u256) && oldRow.Data == 10 && newRow.N.Equals(u256) && newRow.Data == 20, "Unexpected pk_u256 update"); Seen(); };
    test.Db.Db.PkI8.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.N == -1 && oldRow.Data == 10 && newRow.N == -1 && newRow.Data == 20, "Unexpected pk_i8 update"); Seen(); };
    test.Db.Db.PkI16.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.N == -2 && oldRow.Data == 10 && newRow.N == -2 && newRow.Data == 20, "Unexpected pk_i16 update"); Seen(); };
    test.Db.Db.PkI32.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.N == -3 && oldRow.Data == 10 && newRow.N == -3 && newRow.Data == 20, "Unexpected pk_i32 update"); Seen(); };
    test.Db.Db.PkI64.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.N == -4 && oldRow.Data == 10 && newRow.N == -4 && newRow.Data == 20, "Unexpected pk_i64 update"); Seen(); };
    test.Db.Db.PkI128.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.N.Equals(i128) && oldRow.Data == 10 && newRow.N.Equals(i128) && newRow.Data == 20, "Unexpected pk_i128 update"); Seen(); };
    test.Db.Db.PkI256.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.N.Equals(i256) && oldRow.Data == 10 && newRow.N.Equals(i256) && newRow.Data == 20, "Unexpected pk_i256 update"); Seen(); };
    test.Db.Db.PkBool.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.B && oldRow.Data == 10 && newRow.B && newRow.Data == 20, "Unexpected pk_bool update"); Seen(); };
    test.Db.Db.PkString.OnUpdate += (_, oldRow, newRow) => { Require(oldRow.S == "key" && oldRow.Data == 10 && newRow.S == "key" && newRow.Data == 20, "Unexpected pk_string update"); Seen(); };

    test.Db.Reducers.InsertPkU8(1, 10);
    test.Db.Reducers.InsertPkU16(2, 10);
    test.Db.Reducers.InsertPkU32(3, 10);
    test.Db.Reducers.InsertPkU64(4, 10);
    test.Db.Reducers.InsertPkU128(u128, 10);
    test.Db.Reducers.InsertPkU256(u256, 10);
    test.Db.Reducers.InsertPkI8(-1, 10);
    test.Db.Reducers.InsertPkI16(-2, 10);
    test.Db.Reducers.InsertPkI32(-3, 10);
    test.Db.Reducers.InsertPkI64(-4, 10);
    test.Db.Reducers.InsertPkI128(i128, 10);
    test.Db.Reducers.InsertPkI256(i256, 10);
    test.Db.Reducers.InsertPkBool(true, 10);
    test.Db.Reducers.InsertPkString("key", 10);
    test.FrameTickUntil(() => test.Db.Db.PkString.Count == 1);

    test.Db.Reducers.UpdatePkU8(1, 20);
    test.Db.Reducers.UpdatePkU16(2, 20);
    test.Db.Reducers.UpdatePkU32(3, 20);
    test.Db.Reducers.UpdatePkU64(4, 20);
    test.Db.Reducers.UpdatePkU128(u128, 20);
    test.Db.Reducers.UpdatePkU256(u256, 20);
    test.Db.Reducers.UpdatePkI8(-1, 20);
    test.Db.Reducers.UpdatePkI16(-2, 20);
    test.Db.Reducers.UpdatePkI32(-3, 20);
    test.Db.Reducers.UpdatePkI64(-4, 20);
    test.Db.Reducers.UpdatePkI128(i128, 20);
    test.Db.Reducers.UpdatePkI256(i256, 20);
    test.Db.Reducers.UpdatePkBool(true, 20);
    test.Db.Reducers.UpdatePkString("key", 20);
    test.FrameTickUntil(() => remaining == 0);
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
    var reducerSeen = false;
    test.Db.Reducers.OnInsertCallerOneIdentity += ctx =>
    {
        RequireCommitted(ctx.Event.Status);
        reducerSeen = true;
    };

    test.Db.Db.OneIdentity.OnInsert += (_, row) =>
    {
        Require(row.I == test.Identity, "Inserted caller identity did not match connection identity");
        inserted = true;
    };

    test.Db.Reducers.InsertCallerOneIdentity();
    test.FrameTickUntil(() => inserted && reducerSeen);
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
    var reducerSeen = false;
    test.Db.Reducers.OnInsertCallerOneConnectionId += ctx =>
    {
        RequireCommitted(ctx.Event.Status);
        reducerSeen = true;
    };
    test.Db.Db.OneConnectionId.OnInsert += (_, row) =>
    {
        Require(row.A == test.Db.ConnectionId, "Caller ConnectionId did not match connection state");
        inserted = true;
    };
    test.Db.Reducers.InsertCallerOneConnectionId();
    test.FrameTickUntil(() => inserted && reducerSeen);
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
    var reducerSeen = false;
    test.Db.Reducers.OnInsertCallTimestamp += ctx =>
    {
        RequireCommitted(ctx.Event.Status);
        reducerSeen = true;
    };
    test.Db.Db.OneTimestamp.OnInsert += (ctx, row) =>
    {
        Require(ctx.Event is Event<Reducer>.Reducer, "Expected reducer event for insert_call_timestamp");
        Require(row.T.MicrosecondsSinceUnixEpoch > 0, "Reducer timestamp was not populated");
        inserted = true;
    };
    test.Db.Reducers.InsertCallTimestamp();
    test.FrameTickUntil(() => inserted && reducerSeen);
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

void RunInsertCallUuidV4() => RunGeneratedUuid(
    test => test.Db.Reducers.InsertCallUuidV4(),
    (test, mark) => test.Db.Reducers.OnInsertCallUuidV4 += ctx =>
    {
        RequireCommitted(ctx.Event.Status);
        mark();
    });

void RunInsertCallUuidV7() => RunGeneratedUuid(
    test => test.Db.Reducers.InsertCallUuidV7(),
    (test, mark) => test.Db.Reducers.OnInsertCallUuidV7 += ctx =>
    {
        RequireCommitted(ctx.Event.Status);
        mark();
    });

void RunGeneratedUuid(Action<HarnessConnection> callReducer, Action<HarnessConnection, Action> registerReducerCallback)
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.OneUuid()));
    var inserted = false;
    var reducerSeen = false;
    registerReducerCallback(test, () => reducerSeen = true);
    test.Db.Db.OneUuid.OnInsert += (_, row) =>
    {
        Require(row.U != Uuid.NIL, "Generated UUID was nil");
        inserted = true;
    };
    callReducer(test);
    test.FrameTickUntil(() => inserted && reducerSeen);
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
    using var test = ConnectAndSubscribeAll();
    var u128 = new List<U128> { new(0, 5), new(0, 6) };
    var u256 = new List<U256> { new(new U128(0, 0), new U128(0, 7)), new(new U128(0, 0), new U128(0, 8)) };
    var i128 = new List<I128> { new(0, 9), new(0, 10) };
    var i256 = new List<I256> { new(new U128(0, 0), new U128(0, 11)), new(new U128(0, 0), new U128(0, 12)) };
    var uuid = Uuid.Parse("01890f3d-8120-7cc8-9a1f-cd1224fb3a10");
    var timestamp = new Timestamp(1_234_567);
    var duration = new TimeDuration(9_876);
    var remaining = 19;
    void Seen() => remaining--;

    test.Db.Db.VecU8.OnInsert += (_, row) => { RequireSequenceEqual(row.N, new byte[] { 0, 1 }, "VecU8 did not round-trip"); Seen(); };
    test.Db.Db.VecU16.OnInsert += (_, row) => { RequireSequenceEqual(row.N, new ushort[] { 0, 1 }, "VecU16 did not round-trip"); Seen(); };
    test.Db.Db.VecU32.OnInsert += (_, row) => { RequireSequenceEqual(row.N, new uint[] { 0, 1 }, "VecU32 did not round-trip"); Seen(); };
    test.Db.Db.VecU64.OnInsert += (_, row) => { RequireSequenceEqual(row.N, new ulong[] { 0, 1 }, "VecU64 did not round-trip"); Seen(); };
    test.Db.Db.VecU128.OnInsert += (_, row) => { RequireSequenceEqual(row.N, u128, "VecU128 did not round-trip"); Seen(); };
    test.Db.Db.VecU256.OnInsert += (_, row) => { RequireSequenceEqual(row.N, u256, "VecU256 did not round-trip"); Seen(); };
    test.Db.Db.VecI8.OnInsert += (_, row) => { RequireSequenceEqual(row.N, new sbyte[] { 0, 1 }, "VecI8 did not round-trip"); Seen(); };
    test.Db.Db.VecI16.OnInsert += (_, row) => { RequireSequenceEqual(row.N, new short[] { 0, 1 }, "VecI16 did not round-trip"); Seen(); };
    test.Db.Db.VecI32.OnInsert += (_, row) => { RequireSequenceEqual(row.N, new[] { -1, 0, 42 }, "VecI32 did not round-trip"); Seen(); };
    test.Db.Db.VecI64.OnInsert += (_, row) => { RequireSequenceEqual(row.N, new long[] { 0, 1 }, "VecI64 did not round-trip"); Seen(); };
    test.Db.Db.VecI128.OnInsert += (_, row) => { RequireSequenceEqual(row.N, i128, "VecI128 did not round-trip"); Seen(); };
    test.Db.Db.VecI256.OnInsert += (_, row) => { RequireSequenceEqual(row.N, i256, "VecI256 did not round-trip"); Seen(); };
    test.Db.Db.VecBool.OnInsert += (_, row) => { RequireSequenceEqual(row.B, new[] { false, true }, "VecBool did not round-trip"); Seen(); };
    test.Db.Db.VecF32.OnInsert += (_, row) => { Require(row.F.Count == 2 && Math.Abs(row.F[0] - 0.0f) < 0.001f && Math.Abs(row.F[1] - 1.0f) < 0.001f, "VecF32 did not round-trip"); Seen(); };
    test.Db.Db.VecF64.OnInsert += (_, row) => { Require(row.F.Count == 2 && Math.Abs(row.F[0] - 0.0) < 0.001 && Math.Abs(row.F[1] - 1.0) < 0.001, "VecF64 did not round-trip"); Seen(); };
    test.Db.Db.VecString.OnInsert += (_, row) => { RequireSequenceEqual(row.S, new[] { "zero", "one" }, "VecString did not round-trip"); Seen(); };
    test.Db.Db.VecIdentity.OnInsert += (_, row) => { RequireSequenceEqual(row.I, new[] { test.Identity }, "VecIdentity did not round-trip"); Seen(); };
    test.Db.Db.VecConnectionId.OnInsert += (_, row) => { RequireSequenceEqual(row.A, new[] { test.Db.ConnectionId }, "VecConnectionId did not round-trip"); Seen(); };
    test.Db.Db.VecTimestamp.OnInsert += (_, row) => { RequireSequenceEqual(row.T, new[] { timestamp }, "VecTimestamp did not round-trip"); Seen(); };

    test.Db.Reducers.InsertVecU8(new() { 0, 1 });
    test.Db.Reducers.InsertVecU16(new() { 0, 1 });
    test.Db.Reducers.InsertVecU32(new() { 0, 1 });
    test.Db.Reducers.InsertVecU64(new() { 0, 1 });
    test.Db.Reducers.InsertVecU128(u128);
    test.Db.Reducers.InsertVecU256(u256);
    test.Db.Reducers.InsertVecI8(new() { 0, 1 });
    test.Db.Reducers.InsertVecI16(new() { 0, 1 });
    test.Db.Reducers.InsertVecI32(new() { -1, 0, 42 });
    test.Db.Reducers.InsertVecI64(new() { 0, 1 });
    test.Db.Reducers.InsertVecI128(i128);
    test.Db.Reducers.InsertVecI256(i256);
    test.Db.Reducers.InsertVecBool(new() { false, true });
    test.Db.Reducers.InsertVecF32(new() { 0.0f, 1.0f });
    test.Db.Reducers.InsertVecF64(new() { 0.0, 1.0 });
    test.Db.Reducers.InsertVecString(new() { "zero", "one" });
    test.Db.Reducers.InsertVecIdentity(new() { test.Identity });
    test.Db.Reducers.InsertVecConnectionId(new() { test.Db.ConnectionId });
    test.Db.Reducers.InsertVecTimestamp(new() { timestamp });
    test.FrameTickUntil(() => remaining == 0);
}

void RunInsertOptionSome()
{
    using var test = ConnectAndSubscribeAll();
    var primitive = EveryPrimitiveStructValue(test);
    var vecOption = new List<int?> { 0, null };
    var remaining = 6;
    void Seen() => remaining--;

    test.Db.Db.OptionI32.OnInsert += (_, row) => { Require(row.N == 42, "OptionI32 Some did not round-trip"); Seen(); };
    test.Db.Db.OptionString.OnInsert += (_, row) => { Require(row.S == "string", "OptionString Some did not round-trip"); Seen(); };
    test.Db.Db.OptionIdentity.OnInsert += (_, row) => { Require(row.I == test.Identity, "OptionIdentity Some did not round-trip"); Seen(); };
    test.Db.Db.OptionSimpleEnum.OnInsert += (_, row) => { Require(row.E == SimpleEnum.Zero, "OptionSimpleEnum Some did not round-trip"); Seen(); };
    test.Db.Db.OptionEveryPrimitiveStruct.OnInsert += (_, row) =>
    {
        Require(row.S != null, "OptionEveryPrimitiveStruct Some did not round-trip");
        RequireEveryPrimitiveStructEqual(row.S, primitive, "OptionEveryPrimitiveStruct Some did not round-trip");
        Seen();
    };
    test.Db.Db.OptionVecOptionI32.OnInsert += (_, row) => { Require(row.V != null && row.V.SequenceEqual(vecOption), "OptionVecOptionI32 Some did not round-trip"); Seen(); };

    test.Db.Reducers.InsertOptionI32(42);
    test.Db.Reducers.InsertOptionString("string");
    test.Db.Reducers.InsertOptionIdentity(test.Identity);
    test.Db.Reducers.InsertOptionSimpleEnum(SimpleEnum.Zero);
    test.Db.Reducers.InsertOptionEveryPrimitiveStruct(primitive);
    test.Db.Reducers.InsertOptionVecOptionI32(vecOption);
    test.FrameTickUntil(() => remaining == 0);
}

void RunInsertOptionNone()
{
    using var test = ConnectAndSubscribeAll();
    var remaining = 6;
    void Seen() => remaining--;

    test.Db.Db.OptionI32.OnInsert += (_, row) => { Require(row.N == null, "OptionI32 None did not round-trip"); Seen(); };
    test.Db.Db.OptionString.OnInsert += (_, row) => { Require(row.S == null, "OptionString None did not round-trip"); Seen(); };
    test.Db.Db.OptionIdentity.OnInsert += (_, row) => { Require(row.I == null, "OptionIdentity None did not round-trip"); Seen(); };
    test.Db.Db.OptionSimpleEnum.OnInsert += (_, row) => { Require(row.E == null, "OptionSimpleEnum None did not round-trip"); Seen(); };
    test.Db.Db.OptionEveryPrimitiveStruct.OnInsert += (_, row) => { Require(row.S == null, "OptionEveryPrimitiveStruct None did not round-trip"); Seen(); };
    test.Db.Db.OptionVecOptionI32.OnInsert += (_, row) => { Require(row.V == null, "OptionVecOptionI32 None did not round-trip"); Seen(); };

    test.Db.Reducers.InsertOptionI32(null);
    test.Db.Reducers.InsertOptionString(null);
    test.Db.Reducers.InsertOptionIdentity(null);
    test.Db.Reducers.InsertOptionSimpleEnum(null);
    test.Db.Reducers.InsertOptionEveryPrimitiveStruct(null);
    test.Db.Reducers.InsertOptionVecOptionI32(null);
    test.FrameTickUntil(() => remaining == 0);
}

void RunInsertStruct()
{
    using var test = ConnectAndSubscribeAll();
    var primitive = EveryPrimitiveStructValue(test);
    var vec = EveryVecStructValue(test);
    var byteStruct = new ByteStruct { B = 99 };
    var expected = new HashSet<string>
    {
        "one_unit_struct",
        "one_byte_struct",
        "one_every_primitive_struct",
        "one_every_vec_struct",
        "vec_unit_struct",
        "vec_byte_struct",
        "vec_every_primitive_struct",
        "vec_every_vec_struct",
    };
    var seen = new HashSet<string>();
    var oneUnitStructReducerSeen = false;
    void Seen(string name)
    {
        Require(seen.Add(name), $"{name} callback fired more than once");
    }

    test.Db.Reducers.OnInsertOneUnitStruct += (ctx, s) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(s is not null, "InsertOneUnitStruct reducer callback saw null argument");
        oneUnitStructReducerSeen = true;
    };
    test.Db.Db.OneUnitStruct.OnInsert += (_, row) => { Require(row.S is not null, "UnitStruct did not round-trip"); Seen("one_unit_struct"); };
    test.Db.Db.OneByteStruct.OnInsert += (_, row) => { RequireByteStructEqual(row.S, byteStruct, "ByteStruct did not round-trip"); Seen("one_byte_struct"); };
    test.Db.Db.OneEveryPrimitiveStruct.OnInsert += (_, row) => { RequireEveryPrimitiveStructEqual(row.S, primitive, "EveryPrimitiveStruct did not round-trip"); Seen("one_every_primitive_struct"); };
    test.Db.Db.OneEveryVecStruct.OnInsert += (_, row) => { RequireEveryVecStructEqual(row.S, vec, "EveryVecStruct did not round-trip"); Seen("one_every_vec_struct"); };
    test.Db.Db.VecUnitStruct.OnInsert += (_, row) => { RequireSequenceEqual(row.S, new[] { new UnitStruct() }, "VecUnitStruct did not round-trip"); Seen("vec_unit_struct"); };
    test.Db.Db.VecByteStruct.OnInsert += (_, row) => { RequireStructListEqual(row.S, new[] { byteStruct }, RequireByteStructEqual, "VecByteStruct did not round-trip"); Seen("vec_byte_struct"); };
    test.Db.Db.VecEveryPrimitiveStruct.OnInsert += (_, row) => { RequireStructListEqual(row.S, new[] { primitive }, RequireEveryPrimitiveStructEqual, "VecEveryPrimitiveStruct did not round-trip"); Seen("vec_every_primitive_struct"); };
    test.Db.Db.VecEveryVecStruct.OnInsert += (_, row) => { RequireStructListEqual(row.S, new[] { vec }, RequireEveryVecStructEqual, "VecEveryVecStruct did not round-trip"); Seen("vec_every_vec_struct"); };

    test.Db.Reducers.InsertOneUnitStruct(new UnitStruct());
    test.Db.Reducers.InsertOneByteStruct(byteStruct);
    test.Db.Reducers.InsertOneEveryPrimitiveStruct(primitive);
    test.Db.Reducers.InsertOneEveryVecStruct(vec);
    test.Db.Reducers.InsertVecUnitStruct(new() { new UnitStruct() });
    test.Db.Reducers.InsertVecByteStruct(new() { byteStruct });
    test.Db.Reducers.InsertVecEveryPrimitiveStruct(new() { primitive });
    test.Db.Reducers.InsertVecEveryVecStruct(new() { vec });
    test.FrameTickUntil(
        () => seen.SetEquals(expected) && oneUnitStructReducerSeen,
        timeoutMessage: () => $"Timed out waiting for struct callbacks. Missing: {string.Join(", ", expected.Except(seen).OrderBy(name => name))}");
}

void RunInsertSimpleEnum()
{
    using var test = ConnectAndSubscribeAll();
    var remaining = 2;
    void Seen() => remaining--;

    test.Db.Db.OneSimpleEnum.OnInsert += (_, row) =>
    {
        Require(row.E == SimpleEnum.One, "SimpleEnum did not round-trip");
        Seen();
    };
    test.Db.Db.VecSimpleEnum.OnInsert += (_, row) =>
    {
        RequireSequenceEqual(row.E, new[] { SimpleEnum.Zero, SimpleEnum.One, SimpleEnum.Two }, "VecSimpleEnum did not round-trip");
        Seen();
    };

    test.Db.Reducers.InsertOneSimpleEnum(SimpleEnum.One);
    test.Db.Reducers.InsertVecSimpleEnum(new() { SimpleEnum.Zero, SimpleEnum.One, SimpleEnum.Two });
    test.FrameTickUntil(() => remaining == 0);
}

void RunInsertEnumWithPayload()
{
    using var test = ConnectAndSubscribeAll();
    var onePayload = new EnumWithPayload.U8(17);
    var payloads = EnumPayloadValues(test);
    var remaining = 2;
    void Seen() => remaining--;

    test.Db.Db.OneEnumWithPayload.OnInsert += (_, row) =>
    {
        Require(row.E == onePayload, "EnumWithPayload did not round-trip");
        Seen();
    };
    test.Db.Db.VecEnumWithPayload.OnInsert += (_, row) =>
    {
        RequireEnumPayloadListEqual(row.E, payloads, "VecEnumWithPayload did not round-trip");
        Seen();
    };

    test.Db.Reducers.InsertOneEnumWithPayload(onePayload);
    test.Db.Reducers.InsertVecEnumWithPayload(payloads);
    test.FrameTickUntil(() => remaining == 0);
}

void RunInsertDeleteLargeTable()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.LargeTable()));
    var large = LargeTableValue(test);
    var inserted = false;
    var deleted = false;
    var insertReducer = false;
    var deleteReducer = false;
    test.Db.Reducers.OnInsertLargeTable += (
        ctx,
        _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _) =>
    {
        RequireCommitted(ctx.Event.Status);
        insertReducer = true;
    };
    test.Db.Reducers.OnDeleteLargeTable += (
        ctx,
        _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _) =>
    {
        RequireCommitted(ctx.Event.Status);
        deleteReducer = true;
    };
    test.Db.Db.LargeTable.OnInsert += (_, row) =>
    {
        RequireLargeTableEqual(row, large, "LargeTable insert did not round-trip");
        inserted = true;
        CallDeleteLargeTable(test, large);
    };
    test.Db.Db.LargeTable.OnDelete += (_, row) =>
    {
        RequireLargeTableEqual(row, large, "LargeTable delete did not round-trip");
        deleted = true;
    };
    CallInsertLargeTable(test, large);
    test.FrameTickUntil(() => inserted && deleted && insertReducer && deleteReducer);
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
    var reducerSeen = false;
    test.Db.Reducers.OnInsertPrimitivesAsStrings += (ctx, row) =>
    {
        RequireCommitted(ctx.Event.Status);
        RequireEveryPrimitiveStructEqual(row, primitive, "InsertPrimitivesAsStrings reducer callback saw wrong argument");
        reducerSeen = true;
    };
    test.Db.Db.VecString.OnInsert += (_, row) =>
    {
        Require(row.S.SequenceEqual(expected), "Primitive string conversion did not round-trip");
        inserted = true;
    };
    test.Db.Reducers.InsertPrimitivesAsStrings(primitive);
    test.FrameTickUntil(() => inserted && reducerSeen);
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
    var compositeReducerSeen = false;
    test.Db.Reducers.OnInsertUniqueU32UpdatePkU32 += (ctx, n, uniqueData, pkData) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(n == 42 && uniqueData == 0xbeef && pkData == 100, "Unexpected insert_unique_u32_update_pk_u32 args");
        compositeReducerSeen = true;
    };
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
    test.FrameTickUntil(() => pkInsert && pkUpdate && uniqueInsert && compositeReducerSeen);
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
    var compositeReducerSeen = false;
    var uniqueInserts = 0;
    test.Db.Reducers.InsertUniqueU32(42, 0xbeef);
    test.FrameTickUntil(() => true);
    test.Db.Reducers.OnDeletePkU32InsertPkU32Two += (ctx, n, data) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(n == 42 && data == 0xbeef, "Unexpected delete_pk_u32_insert_pk_u32_two args");
        compositeReducerSeen = true;
    };
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
    test.FrameTickUntil(() => pkInsert && pkDelete && pkTwoInsert && compositeReducerSeen);
    Require(uniqueInserts == 1, $"Expected exactly one deduplicated unique_u32 insert, got {uniqueInserts}");
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
    var insertPkReducers = 0;
    var insertUniqueReducers = 0;
    var updateReducers = 0;
    test.Db.Reducers.OnInsertPkU32 += (ctx, n, data) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require((n == 1 || n == 2) && data == 0, "Unexpected insert_pk_u32 args");
        insertPkReducers++;
    };
    test.Db.Reducers.OnInsertUniqueU32 += (ctx, n, data) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require((n == 1 && data == 3) || (n == 2 && data == 4), "Unexpected insert_unique_u32 args");
        insertUniqueReducers++;
    };
    test.Db.Reducers.OnUpdatePkU32 += (ctx, n, data) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(n == 2 && data is 0 or 1, "Unexpected update_pk_u32 args");
        updateReducers++;
    };
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
    test.FrameTickUntil(() => insertedRows == 2 && insertPkReducers == 2 && insertUniqueReducers == 2);
    test.Db.Reducers.UpdatePkU32(2, 1);
    test.FrameTickUntil(() => update1 && update2 && updateReducers == 2);
}

void RunIntraQueryBagSemanticsForJoin()
{
    using var test = ConnectAndSubscribeSql(
        "SELECT * FROM btree_u32",
        "SELECT pk_u32.* FROM pk_u32 JOIN btree_u32 ON pk_u32.n = btree_u32.n");
    var pkInserts = 0;
    var pkDeletes = 0;
    var insertBtreeReducerSeen = false;
    var insertPkBtreeReducerSeen = false;
    var firstBtreeDeleteReducerSeen = false;
    var secondBtreeDeleteReducerSeen = false;

    test.Db.Reducers.OnInsertIntoBtreeU32 += (ctx, rows) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(rows.Count == 1 && rows[0].N == 0 && rows[0].Data == 0, "Unexpected insert_into_btree_u32 args");
        insertBtreeReducerSeen = true;
    };
    test.Db.Reducers.OnInsertIntoPkBtreeU32 += (ctx, pkRows, btreeRows) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(pkRows.Count == 1 && pkRows[0].N == 0 && pkRows[0].Data == 0, "Unexpected insert_into_pk_btree_u32 pk args");
        Require(btreeRows.Count == 1 && btreeRows[0].N == 0 && btreeRows[0].Data == 1, "Unexpected insert_into_pk_btree_u32 btree args");
        insertPkBtreeReducerSeen = true;
    };
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

    test.FrameTickUntil(() => insertBtreeReducerSeen && insertPkBtreeReducerSeen && firstBtreeDeleteReducerSeen && secondBtreeDeleteReducerSeen && pkDeletes == 1);
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
    var aliceReducerSeen = false;
    var bobReducerSeen = false;
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
    alice.Db.Reducers.OnInsertUser += (ctx, name, identity) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(name == "Alice" && identity == alice.Identity, "Unexpected Alice insert_user args");
        aliceReducerSeen = true;
    };
    bob.Db.Reducers.OnInsertUser += (ctx, name, identity) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(name == "Bob" && identity == bob.Identity, "Unexpected Bob insert_user args");
        bobReducerSeen = true;
    };
    alice.Db.Reducers.InsertUser("Alice", alice.Identity);
    bob.Db.Reducers.InsertUser("Bob", bob.Identity);
    FrameTickUntil(new[] { alice, bob }, () => aliceInserted && bobInserted && aliceReducerSeen && bobReducerSeen);
}

void RunPkSimpleEnum()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.PkSimpleEnum()));
    var updated = false;
    var enumValue = SimpleEnum.Two;
    var insertReducerSeen = false;
    var updateReducerSeen = false;
    test.Db.Reducers.OnInsertPkSimpleEnum += (ctx, a, data) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(a == enumValue && data == 42, "Unexpected insert_pk_simple_enum args");
        insertReducerSeen = true;
    };
    test.Db.Reducers.OnUpdatePkSimpleEnum += (ctx, a, data) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(a == enumValue && data == 24, "Unexpected update_pk_simple_enum args");
        updateReducerSeen = true;
    };
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
    test.FrameTickUntil(() => updated && insertReducerSeen && updateReducerSeen);
}

void RunIndexedSimpleEnum()
{
    using var test = ConnectAndSubscribe(conn => conn.SubscriptionBuilder().AddQuery(qb => qb.From.IndexedSimpleEnum()));
    var updated = false;
    var insertReducerSeen = false;
    var updateReducerSeen = false;
    test.Db.Reducers.OnInsertIntoIndexedSimpleEnum += (ctx, n) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(n == SimpleEnum.Two, "Unexpected insert_into_indexed_simple_enum args");
        insertReducerSeen = true;
    };
    test.Db.Reducers.OnUpdateIndexedSimpleEnum += (ctx, oldValue, newValue) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(oldValue == SimpleEnum.Two && newValue == SimpleEnum.One, "Unexpected update_indexed_simple_enum args");
        updateReducerSeen = true;
    };
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
    test.FrameTickUntil(() => updated && insertReducerSeen && updateReducerSeen);
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
    var updateReducerSeen = false;
    test.Db.Reducers.OnUpdatePkU8 += (ctx, n, data) =>
    {
        RequireCommitted(ctx.Event.Status);
        Require(n == 1 && data == 1, "Unexpected update_pk_u8 args");
        updateReducerSeen = true;
    };
    test.Db.Db.PkU8.OnUpdate += (_, oldRow, newRow) =>
    {
        Require(oldRow.N == 1 && oldRow.Data == 0 && newRow.N == 1 && newRow.Data == 1, "Overlapping update was wrong");
        updated = true;
    };
    test.Db.Reducers.UpdatePkU8(1, 1);
    test.FrameTickUntil(() => updated && updateReducerSeen);
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

void RequireByteStructEqual(ByteStruct actual, ByteStruct expected, string message)
{
    Require(actual.B == expected.B, message);
}

void RequireEveryPrimitiveStructEqual(EveryPrimitiveStruct actual, EveryPrimitiveStruct expected, string message)
{
    Require(
        actual.A == expected.A &&
        actual.B == expected.B &&
        actual.C == expected.C &&
        actual.D == expected.D &&
        actual.E == expected.E &&
        actual.F == expected.F &&
        actual.G == expected.G &&
        actual.H == expected.H &&
        actual.I == expected.I &&
        actual.J == expected.J &&
        actual.K == expected.K &&
        actual.L == expected.L &&
        actual.M == expected.M &&
        actual.N == expected.N &&
        actual.O == expected.O &&
        actual.P == expected.P &&
        actual.Q == expected.Q &&
        actual.R == expected.R &&
        actual.S == expected.S &&
        actual.T == expected.T &&
        actual.U == expected.U,
        message);
}

void RequireEveryVecStructEqual(EveryVecStruct actual, EveryVecStruct expected, string message)
{
    Require(
        actual.A.SequenceEqual(expected.A) &&
        actual.B.SequenceEqual(expected.B) &&
        actual.C.SequenceEqual(expected.C) &&
        actual.D.SequenceEqual(expected.D) &&
        actual.E.SequenceEqual(expected.E) &&
        actual.F.SequenceEqual(expected.F) &&
        actual.G.SequenceEqual(expected.G) &&
        actual.H.SequenceEqual(expected.H) &&
        actual.I.SequenceEqual(expected.I) &&
        actual.J.SequenceEqual(expected.J) &&
        actual.K.SequenceEqual(expected.K) &&
        actual.L.SequenceEqual(expected.L) &&
        actual.M.SequenceEqual(expected.M) &&
        actual.N.SequenceEqual(expected.N) &&
        actual.O.SequenceEqual(expected.O) &&
        actual.P.SequenceEqual(expected.P) &&
        actual.Q.SequenceEqual(expected.Q) &&
        actual.R.SequenceEqual(expected.R) &&
        actual.S.SequenceEqual(expected.S) &&
        actual.T.SequenceEqual(expected.T) &&
        actual.U.SequenceEqual(expected.U),
        message);
}

void RequireLargeTableEqual(LargeTable actual, LargeTable expected, string message)
{
    Require(
        actual.A == expected.A &&
        actual.B == expected.B &&
        actual.C == expected.C &&
        actual.D == expected.D &&
        actual.E == expected.E &&
        actual.F == expected.F &&
        actual.G == expected.G &&
        actual.H == expected.H &&
        actual.I == expected.I &&
        actual.J == expected.J &&
        actual.K == expected.K &&
        actual.L == expected.L &&
        actual.M == expected.M &&
        actual.N == expected.N &&
        actual.O == expected.O &&
        actual.P == expected.P &&
        actual.Q == expected.Q,
        message);
    RequireEnumPayloadEqual(actual.R, expected.R, message);
    RequireByteStructEqual(actual.T, expected.T, message);
    RequireEveryPrimitiveStructEqual(actual.U, expected.U, message);
    RequireEveryVecStructEqual(actual.V, expected.V, message);
}

void RequireStructListEqual<T>(IReadOnlyList<T> actual, IReadOnlyList<T> expected, Action<T, T, string> requireEqual, string message)
{
    Require(actual.Count == expected.Count, message);
    for (var i = 0; i < actual.Count; i++)
    {
        requireEqual(actual[i], expected[i], message);
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

List<EnumWithPayload> EnumPayloadValues(HarnessConnection test) => new()
{
    new EnumWithPayload.U8(0),
    new EnumWithPayload.U16(1),
    new EnumWithPayload.U32(2),
    new EnumWithPayload.U64(3),
    new EnumWithPayload.U128(new U128(0, 4)),
    new EnumWithPayload.U256(new U256(new U128(0, 0), new U128(0, 5))),
    new EnumWithPayload.I8(0),
    new EnumWithPayload.I16(-1),
    new EnumWithPayload.I32(-2),
    new EnumWithPayload.I64(-3),
    new EnumWithPayload.I128(new I128(0, 4)),
    new EnumWithPayload.I256(new I256(new U128(0, 0), new U128(0, 5))),
    new EnumWithPayload.Bool(true),
    new EnumWithPayload.F32(0.0f),
    new EnumWithPayload.F64(100.0),
    new EnumWithPayload.Str("enum holds string"),
    new EnumWithPayload.Identity(test.Identity),
    new EnumWithPayload.ConnectionId(test.Db.ConnectionId),
    new EnumWithPayload.Timestamp(new Timestamp(1_234_567)),
    new EnumWithPayload.Uuid(Uuid.Parse("01890f3d-8120-7cc8-9a1f-cd1224fb3a10")),
    new EnumWithPayload.Bytes(new() { 0xde, 0xad, 0xbe, 0xef }),
    new EnumWithPayload.Ints(new() { 0, 1, 2 }),
    new EnumWithPayload.Strings(new() { "enum", "of", "vec", "of", "strings" }),
    new EnumWithPayload.SimpleEnums(new() { SimpleEnum.Zero, SimpleEnum.One, SimpleEnum.Two }),
};

void RequireSequenceEqual<T>(IEnumerable<T> actual, IEnumerable<T> expected, string message)
{
    if (!actual.SequenceEqual(expected))
    {
        throw new Exception(message);
    }
}

void RequireEnumPayloadListEqual(IReadOnlyList<EnumWithPayload> actual, IReadOnlyList<EnumWithPayload> expected, string message)
{
    Require(actual.Count == expected.Count, message);
    for (var i = 0; i < actual.Count; i++)
    {
        RequireEnumPayloadEqual(actual[i], expected[i], message);
    }
}

void RequireEnumPayloadEqual(EnumWithPayload actual, EnumWithPayload expected, string message)
{
    var equal = (actual, expected) switch
    {
        (EnumWithPayload.U8 a, EnumWithPayload.U8 e) => a.U8_ == e.U8_,
        (EnumWithPayload.U16 a, EnumWithPayload.U16 e) => a.U16_ == e.U16_,
        (EnumWithPayload.U32 a, EnumWithPayload.U32 e) => a.U32_ == e.U32_,
        (EnumWithPayload.U64 a, EnumWithPayload.U64 e) => a.U64_ == e.U64_,
        (EnumWithPayload.U128 a, EnumWithPayload.U128 e) => a.U128_ == e.U128_,
        (EnumWithPayload.U256 a, EnumWithPayload.U256 e) => a.U256_ == e.U256_,
        (EnumWithPayload.I8 a, EnumWithPayload.I8 e) => a.I8_ == e.I8_,
        (EnumWithPayload.I16 a, EnumWithPayload.I16 e) => a.I16_ == e.I16_,
        (EnumWithPayload.I32 a, EnumWithPayload.I32 e) => a.I32_ == e.I32_,
        (EnumWithPayload.I64 a, EnumWithPayload.I64 e) => a.I64_ == e.I64_,
        (EnumWithPayload.I128 a, EnumWithPayload.I128 e) => a.I128_ == e.I128_,
        (EnumWithPayload.I256 a, EnumWithPayload.I256 e) => a.I256_ == e.I256_,
        (EnumWithPayload.Bool a, EnumWithPayload.Bool e) => a.Bool_ == e.Bool_,
        (EnumWithPayload.F32 a, EnumWithPayload.F32 e) => a.F32_ == e.F32_,
        (EnumWithPayload.F64 a, EnumWithPayload.F64 e) => a.F64_ == e.F64_,
        (EnumWithPayload.Str a, EnumWithPayload.Str e) => a.Str_ == e.Str_,
        (EnumWithPayload.Identity a, EnumWithPayload.Identity e) => a.Identity_ == e.Identity_,
        (EnumWithPayload.ConnectionId a, EnumWithPayload.ConnectionId e) => a.ConnectionId_ == e.ConnectionId_,
        (EnumWithPayload.Timestamp a, EnumWithPayload.Timestamp e) => a.Timestamp_ == e.Timestamp_,
        (EnumWithPayload.Uuid a, EnumWithPayload.Uuid e) => a.Uuid_ == e.Uuid_,
        (EnumWithPayload.Bytes a, EnumWithPayload.Bytes e) => a.Bytes_.SequenceEqual(e.Bytes_),
        (EnumWithPayload.Ints a, EnumWithPayload.Ints e) => a.Ints_.SequenceEqual(e.Ints_),
        (EnumWithPayload.Strings a, EnumWithPayload.Strings e) => a.Strings_.SequenceEqual(e.Strings_),
        (EnumWithPayload.SimpleEnums a, EnumWithPayload.SimpleEnums e) => a.SimpleEnums_.SequenceEqual(e.SimpleEnums_),
        _ => false,
    };
    Require(equal, message);
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

    public void FrameTickUntil(Func<bool> isComplete, int timeoutSeconds = 20, Func<string>? timeoutMessage = null)
    {
        var deadline = DateTime.UtcNow.AddSeconds(timeoutSeconds);
        while (!isComplete())
        {
            Db.FrameTick();
            Thread.Sleep(25);
            if (DateTime.UtcNow > deadline)
            {
                throw new TimeoutException(timeoutMessage?.Invoke() ?? $"Timed out after {timeoutSeconds} seconds");
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
