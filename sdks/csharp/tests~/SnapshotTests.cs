namespace SpacetimeDB.Tests;

using System.IO.Compression;
using System.Reflection;
using System.Threading;
using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;
using SpacetimeDB.Types;
using Xunit;

using U128 = SpacetimeDB.U128;

public class SnapshotTests
{
    sealed class TestSubscriptionHandle : ISubscriptionHandle
    {
        public void OnApplied(ISubscriptionEventContext ctx) { }
        public void OnError(IErrorContext ctx) { }
        public void OnEnded(ISubscriptionEventContext ctx) { }
    }

    class Events : List<KeyValuePair<string, object?>>
    {
        private bool frozen;

        public void Add(string name, object? value = null)
        {
            if (frozen)
            {
                throw new InvalidOperationException("This is a bug. We have snapshotted the events and don't expect any more to arrive.");
            }
            base.Add(new(name, value));
        }

        public void Freeze()
        {
            frozen = true;
        }
    }

    class EventsConverter : WriteOnlyJsonConverter<Events>
    {
        public override void Write(VerifyJsonWriter writer, Events events)
        {
            writer.WriteStartObject();
            foreach (var (name, value) in events)
            {
                writer.WriteMember(events, value, name);
            }
            writer.WriteEndObject();
        }
    }

    class TimestampConverter : WriteOnlyJsonConverter<Timestamp>
    {
        public override void Write(VerifyJsonWriter writer, Timestamp timestamp)
        {
            writer.WriteValue(timestamp.MicrosecondsSinceUnixEpoch);
        }
    }

    class EnergyQuantaConverter : WriteOnlyJsonConverter<EnergyQuanta>
    {
        public override void Write(VerifyJsonWriter writer, EnergyQuanta value)
        {
            writer.WriteRawValueIfNoStrict(value.Quanta.ToString());
        }
    }

    class TestLogger(Events events) : ISpacetimeDBLogger
    {
        public void Debug(string message)
        {
            events.Add("Debug", message);
        }

        public void Trace(string message)
        {
            events.Add("Trace", message);
        }

        public void Info(string message)
        {
            events.Add("Log", message);
        }

        public void Warn(string message)
        {
            events.Add("LogWarning", message);
        }

        public void Error(string message)
        {
            events.Add("LogError", message);
        }

        public void Exception(string message)
        {
            events.Add("LogException", message);
        }

        public void Exception(Exception e)
        {
            events.Add("LogException", e.Message);
        }
    }

    private static ServerMessage.InitialConnection SampleId(string identity, string token, string address) =>
        new(new()
        {
            Identity = Identity.From(Convert.FromBase64String(identity)),
            Token = token,
            ConnectionId = ConnectionId.From(Convert.FromBase64String(address)) ?? throw new InvalidDataException("address")
        });

    private static ServerMessage.SubscribeApplied SampleSubscribeApplied(
        uint requestId,
        uint querySetId,
        List<SingleTableRows> tables
    ) => new(new()
    {
        RequestId = requestId,
        QuerySetId = new(querySetId),
        Rows = new QueryRows
        {
            Tables = tables
        }
    });

    private static ServerMessage.UnsubscribeApplied SampleUnsubscribeApplied(
        uint requestId,
        uint querySetId,
        List<SingleTableRows>? tables
    ) => new(new()
    {
        RequestId = requestId,
        QuerySetId = new(querySetId),
        Rows = tables == null ? null : new QueryRows { Tables = tables }
    });

    private static ServerMessage.SubscriptionError SampleSubscriptionError(
        uint? requestId,
        uint querySetId,
        string error
    ) => new(new()
    {
        RequestId = requestId,
        QuerySetId = new(querySetId),
        Error = error,
    });

    private static ServerMessage.TransactionUpdate SampleTransactionUpdate(
        uint querySetId,
        List<TableUpdate> updates
    ) => new(new()
    {
        QuerySets = new()
        {
            new QuerySetUpdate
            {
                QuerySetId = new(querySetId),
                Tables = updates
            }
        },
    });

    private static ServerMessage.ReducerResult SampleReducerResultOk(
        uint requestId,
        long timestampMicros,
        TransactionUpdate update
    ) => new(new()
    {
        RequestId = requestId,
        Timestamp = new Timestamp(timestampMicros),
        Result = new ReducerOutcome.Ok(new ReducerOk
        {
            RetValue = [],
            TransactionUpdate = update
        }),
    });

    private static ServerMessage.ReducerResult SampleReducerResultErr(
        uint requestId,
        long timestampMicros,
        string error
    ) => new(new()
    {
        RequestId = requestId,
        Timestamp = new Timestamp(timestampMicros),
        Result = new ReducerOutcome.Err(EncodeBsatnString(error)),
    });

    private static ServerMessage.ReducerResult SampleReducerResultInternalError(
        uint requestId,
        long timestampMicros,
        string error
    ) => new(new()
    {
        RequestId = requestId,
        Timestamp = new Timestamp(timestampMicros),
        Result = new ReducerOutcome.InternalError(error),
    });

    private static TableUpdate SampleUpdate<T>(
        string tableName,
        List<T> inserts,
        List<T> deletes
    ) where T : IStructuralReadWrite => new()
    {
        TableName = tableName,
        Rows =
        [
            new TableUpdateRows.PersistentTable(new PersistentTableRows(
                EncodeRowList<T>(inserts),
                EncodeRowList<T>(deletes)
            ))
        ]
    };

    private static SingleTableRows SampleRows<T>(string tableName, List<T> rows)
        where T : IStructuralReadWrite => new()
        {
            Table = tableName,
            Rows = EncodeRowList(rows)
        };

    private static BsatnRowList EncodeRowList<T>(in List<T> list) where T : IStructuralReadWrite
    {
        var offsets = new List<ulong>();
        var stream = new MemoryStream();
        var writer = new BinaryWriter(stream);
        foreach (var elem in list)
        {
            offsets.Add((ulong)stream.Length);
            elem.WriteFields(writer);
        }
        return new BsatnRowList
        {
            RowsData = stream.ToArray().ToList(),
            SizeHint = new RowSizeHint.RowOffsets(offsets)
        };
    }

    private static byte[] Encode<T>(in T value) where T : IStructuralReadWrite
    {
        var o = new MemoryStream();
        var w = new BinaryWriter(o);
        value.WriteFields(w);
        return o.ToArray();
    }

    private static List<byte> EncodeBsatnString(string value)
    {
        var o = new MemoryStream();
        var w = new BinaryWriter(o);
        new SpacetimeDB.BSATN.String().Write(w, value);
        return [.. o.ToArray()];
    }

    private static readonly string USER_TABLE_NAME = "User";
    private static readonly string MESSAGE_TABLE_NAME = "Message";


    private static TableUpdate SampleUserInsert(string identity, string? name, bool online) =>
        SampleUpdate(USER_TABLE_NAME, [new User
        {
            Identity = Identity.From(Convert.FromBase64String(identity)),
            Name = name,
            Online = online
        }], []);

    private static SingleTableRows SampleUserRows(string identity, string? name, bool online) =>
        SampleRows(USER_TABLE_NAME, [new User
        {
            Identity = Identity.From(Convert.FromBase64String(identity)),
            Name = name,
            Online = online
        }]);

    private static TableUpdate SampleUserUpdate(string identity, string? oldName, string? newName, bool oldOnline, bool newOnline) =>
        SampleUpdate(USER_TABLE_NAME, [new User
        {
            Identity = Identity.From(Convert.FromBase64String(identity)),
            Name = newName,
            Online = newOnline
        }], [new User
        {
            Identity = Identity.From(Convert.FromBase64String(identity)),
            Name = oldName,
            Online = oldOnline
        }]);


    private static Message SampleMessage(string identity, long sentMicrosecondsSinceUnixEpoch, string text) => new()
    {
        Sender = Identity.From(Convert.FromBase64String(identity)),
        Sent = new Timestamp(sentMicrosecondsSinceUnixEpoch),
        Text = text
    };

    private static TableUpdate SampleMessageInsert(List<Message> messages) =>
        SampleUpdate(MESSAGE_TABLE_NAME, messages, []);

    private static SingleTableRows SampleMessageRows(List<Message> messages) =>
        SampleRows(MESSAGE_TABLE_NAME, messages);

    public static IEnumerable<object[]> SampleDump()
    {
        var sampleMessage0 = SampleMessage("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", 1718487775346381, "Hello, A!");
        var sampleMessage1 = SampleMessage("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", 1718487783175083, "Hello, B!");
        var sampleMessage2 = SampleMessage("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", 1718487787645364, "Goodbye!");
        var sampleMessage3 = SampleMessage("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", 1718487794937841, "Goodbye!");

        yield return new object[] { "LegacySubscribeAll",
            new ServerMessage[] {
            SampleId(
                "j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=",
                "eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9.eyJoZXhfaWRlbnRpdHkiOiJjMjAwNDgzMTUyZDY0MmM3ZDQwMmRlMDZjYWNjMzZkY2IwYzJhMWYyYmJlYjhlN2Q1YTY3M2YyNDM1Y2NhOTc1Iiwic3ViIjoiNmQ0YjU0MzAtMDBjZi00YTk5LTkzMmMtYWQyZDA3YmFiODQxIiwiaXNzIjoibG9jYWxob3N0IiwiYXVkIjpbInNwYWNldGltZWRiIl0sImlhdCI6MTczNzY2NTc2OSwiZXhwIjpudWxsfQ.GaKhvswWYW6wpPpK70_-Tw8DKjKJ2qnidwwj1fTUf3mctcsm_UusPYSws_pSW3qGnMNnGjEXt7rRNvGvuWf9ow",
                "Vd4dFzcEzhLHJ6uNL8VXFg=="
            ),
            SampleSubscribeApplied(1, 1, [
                SampleUserRows("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, true),
                SampleMessageRows([])
            ]),
            SampleTransactionUpdate(1, [
                SampleUserInsert("k5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, true)
            ]),
            SampleTransactionUpdate(1, [
                SampleUserInsert("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", null, true)
            ]),
            SampleReducerResultOk(1, 1718487768057579, SampleTransactionUpdate(1, [
                SampleUserUpdate("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, "A", true, true)
            ]).TransactionUpdate_),
            SampleReducerResultErr(2, 1718487770000000, "name cannot be empty"),
            SampleReducerResultInternalError(3, 1718487771000000, "internal reducer failure"),
            SampleTransactionUpdate(1, [
                SampleUserUpdate("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", null, "B", true, true)
            ]),
            SampleTransactionUpdate(1, [SampleMessageInsert([sampleMessage0])]),
            SampleTransactionUpdate(1, [SampleMessageInsert([sampleMessage1])]),
            SampleTransactionUpdate(1, [SampleMessageInsert([sampleMessage2])]),
            SampleTransactionUpdate(1, [
                SampleUserUpdate("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "B", "B", true, false)
            ]),
            SampleTransactionUpdate(1, [SampleMessageInsert([sampleMessage3])]),
            }
        };
        yield return new object[] { "SubscribeApplied",
            new ServerMessage[] {
            SampleId(
                "j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=",
                "eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9.eyJoZXhfaWRlbnRpdHkiOiJjMjAwNDgzMTUyZDY0MmM3ZDQwMmRlMDZjYWNjMzZkY2IwYzJhMWYyYmJlYjhlN2Q1YTY3M2YyNDM1Y2NhOTc1Iiwic3ViIjoiNmQ0YjU0MzAtMDBjZi00YTk5LTkzMmMtYWQyZDA3YmFiODQxIiwiaXNzIjoibG9jYWxob3N0IiwiYXVkIjpbInNwYWNldGltZWRiIl0sImlhdCI6MTczNzY2NTc2OSwiZXhwIjpudWxsfQ.GaKhvswWYW6wpPpK70_-Tw8DKjKJ2qnidwwj1fTUf3mctcsm_UusPYSws_pSW3qGnMNnGjEXt7rRNvGvuWf9ow",
                "Vd4dFzcEzhLHJ6uNL8VXFg=="
            ),
            SampleSubscribeApplied(1, 1, [
                SampleUserRows("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, true),
            ]),
            SampleSubscribeApplied(2, 2, [
                SampleMessageRows([])
            ]),
            SampleTransactionUpdate(1, [SampleUserInsert("k5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, true)]),
            SampleTransactionUpdate(1, [SampleUserInsert("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", null, true)]),
            SampleReducerResultOk(1, 1718487768057579, SampleTransactionUpdate(1, [SampleUserUpdate("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, "A", true, true)]).TransactionUpdate_),
            SampleReducerResultErr(2, 1718487770000000, "name cannot be empty"),
            SampleTransactionUpdate(1, [SampleUserUpdate("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", null, "B", true, true)]),
            SampleTransactionUpdate(2, [SampleMessageInsert([sampleMessage0])]),
            SampleTransactionUpdate(2, [SampleMessageInsert([sampleMessage1])]),
            SampleTransactionUpdate(2, [SampleMessageInsert([sampleMessage2])]),
            SampleTransactionUpdate(1, [SampleUserUpdate("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "B", "B", true, false)]),
            SampleTransactionUpdate(2, [SampleMessageInsert([sampleMessage3])]),
            // Let's pretend the user unsubscribed from the table Messages...
            SampleUnsubscribeApplied(3, 2, [SampleMessageRows([sampleMessage0, sampleMessage1, sampleMessage2, sampleMessage3])]),
            // Subscription failed during recompilation after being applied.
            SampleSubscriptionError(null, 4, "bad query dude"),
            // Then successfully resubscribed.
            SampleSubscribeApplied(4, 4, [SampleMessageRows([sampleMessage0, sampleMessage1, sampleMessage2, sampleMessage3])]),
            // Unsubscribe without requesting dropped rows.
            SampleUnsubscribeApplied(5, 4, null)
            }
        };

    }

    [Theory]
    [MemberData(nameof(SampleDump))]
    public async Task VerifySampleDump(string dumpName, ServerMessage[] sampleDumpParsed)
    {
        var events = new Events();

        Log.Current = new TestLogger(events);

        DbConnection.IsTesting = true;

        var client =
            DbConnection.Builder()
            .WithUri("wss://spacetimedb.com")
            .WithDatabaseName("example")
            .OnConnect((conn, identity, token) => events.Add("OnConnect", new { identity, token }))
            .Build();

        // Snapshot tests inject raw server messages directly, so there is no real outgoing reducer call
        // to populate DbConnectionBase.pendingReducerCalls. Without priming this map, a v2 ReducerResult
        // is treated as unknown request_id and exercises the strict error path instead of reducer callbacks.
        // We use reflection to seed the internal correlation state for deterministic reducer-result coverage.
        static void PrimePendingReducerCall(DbConnection client, uint requestId, Reducer reducer)
        {
            var baseType = client.GetType().BaseType ?? throw new InvalidOperationException("DbConnection has no base type.");
            var pendingCallsField = baseType.GetField("pendingReducerCalls", BindingFlags.Instance | BindingFlags.NonPublic)
                ?? throw new InvalidOperationException("Failed to find pendingReducerCalls field.");
            var pendingCallsObj = pendingCallsField.GetValue(client)
                ?? throw new InvalidOperationException("pendingReducerCalls is null.");
            var pendingCallType = pendingCallsObj.GetType().GetGenericArguments()[1];
            var pendingCall = Activator.CreateInstance(pendingCallType)
                ?? throw new InvalidOperationException("Failed to construct PendingReducerCall.");
            pendingCallType.GetField("Reducer", BindingFlags.Instance | BindingFlags.Public)?.SetValue(pendingCall, reducer);

            var tryAdd = pendingCallsObj.GetType().GetMethod("TryAdd")
                ?? throw new InvalidOperationException("Failed to find TryAdd on pendingReducerCalls.");
            var added = (bool)(tryAdd.Invoke(pendingCallsObj, [requestId, pendingCall]) ?? false);
            if (!added)
            {
                throw new InvalidOperationException($"Failed to prime pending reducer call for request_id={requestId}");
            }
        }

        // Snapshot tests also inject raw subscription server messages directly, so there is no real
        // outgoing Subscribe call to populate DbConnectionBase.subscriptions. Prime known query_set_id
        // entries so SubscribeApplied/UnsubscribeApplied routes through the typed subscription path.
        static void PrimeSubscription(DbConnection client, uint querySetId)
        {
            var baseType = client.GetType().BaseType ?? throw new InvalidOperationException("DbConnection has no base type.");
            var subscriptionsField = baseType.GetField("subscriptions", BindingFlags.Instance | BindingFlags.NonPublic)
                ?? throw new InvalidOperationException("Failed to find subscriptions field.");
            var subscriptionsObj = subscriptionsField.GetValue(client)
                ?? throw new InvalidOperationException("subscriptions is null.");

            var dictType = subscriptionsObj.GetType();
            var containsKey = dictType.GetMethod("ContainsKey")
                ?? throw new InvalidOperationException("Failed to find ContainsKey on subscriptions.");
            if ((bool)(containsKey.Invoke(subscriptionsObj, [querySetId]) ?? false))
            {
                return;
            }

            var add = dictType.GetMethod("Add")
                ?? throw new InvalidOperationException("Failed to find Add on subscriptions.");
            add.Invoke(subscriptionsObj, [querySetId, new TestSubscriptionHandle()]);
        }

        // But for proper testing we need to convert it back to raw binary messages as if it was received over network.
        var sampleDumpBinary = sampleDumpParsed.Select(
            (message, i) =>
            {
                // Start tracking requests in the stats handler so that those request IDs can later be found.
                switch (message)
                {
                    case ServerMessage.SubscribeApplied(var subscribeApplied):
                        client.stats.SubscriptionRequestTracker.StartTrackingRequest($"sample#{i}");
                        PrimeSubscription(client, subscribeApplied.QuerySetId.Id);
                        break;
                    case ServerMessage.UnsubscribeApplied(var _):
                        client.stats.SubscriptionRequestTracker.StartTrackingRequest($"sample#{i}");
                        break;
                    case ServerMessage.SubscriptionError(var s) when s.RequestId.HasValue:
                        client.stats.SubscriptionRequestTracker.StartTrackingRequest($"sample#{i}");
                        break;
                    case ServerMessage.ReducerResult(var reducerResult):
                        {
                            // Keep stats request IDs aligned with synthetic reducer results so tracker snapshots
                            // and correlation behavior match what happens in real client/server traffic.
                            var started = client.stats.ReducerRequestTracker.StartTrackingRequest($"sample#{i}");
                            if (started != reducerResult.RequestId)
                            {
                                throw new InvalidOperationException(
                                    $"Reducer request_id mismatch in sample dump. expected={reducerResult.RequestId}, started={started}"
                                );
                            }
                            if (reducerResult.RequestId == 1)
                            {
                                PrimePendingReducerCall(client, reducerResult.RequestId, new Reducer.SetName { Name = "A" });
                            }
                            else if (reducerResult.RequestId == 2)
                            {
                                PrimePendingReducerCall(client, reducerResult.RequestId, new Reducer.SetName { Name = "" });
                            }
                            else if (reducerResult.RequestId == 3)
                            {
                                PrimePendingReducerCall(client, reducerResult.RequestId, new Reducer.SendMessage { Text = "internal" });
                            }
                            break;
                        }
                }
                using var output = new MemoryStream();
                output.WriteByte(1); // Write compression tag.
                using (var brotli = new BrotliStream(output, CompressionMode.Compress))
                {
                    using var w = new BinaryWriter(brotli);
                    new ServerMessage.BSATN().Write(w, message);
                }
                return output.ToArray();
            }
        );

        client.Db.User.OnDelete += (eventContext, user) =>
            events.Add("OnDeleteUser", new { eventContext, user });
        client.Db.User.OnInsert += (eventContext, user) =>
            events.Add("OnInsertUser", new { eventContext, user });
        client.Db.User.OnUpdate += (eventContext, oldUser, newUser) =>
            events.Add(
                "OnUpdateUser",
                new
                {
                    eventContext,
                    oldUser,
                    newUser
                }
            );

        client.Db.Message.OnDelete += (eventContext, message) =>
            events.Add("OnDeleteMessage", new { eventContext, message });
        client.Db.Message.OnInsert += (eventContext, message) =>
            events.Add("OnInsertMessage", new { eventContext, message });
        client.Reducers.OnSetName += (eventContext, name) =>
            events.Add("OnSetName", new { eventContext, name });
        client.Reducers.OnSendMessage += (eventContext, text) =>
            events.Add("OnSendMessage", new { eventContext, text });

        // Simulate receiving WebSocket messages.
        foreach (var sample in sampleDumpBinary)
        {
            client.OnMessageReceived(sample, DateTime.UtcNow);
            // Wait for this message to be picked up by the background thread, preprocessed and stored in the preprocessed queue.
            // Otherwise we'll get inconsistent output order between test reruns.
            var deadline = DateTime.UtcNow.AddSeconds(2);
            while (!client.HasMessageToApply)
            {
                if (DateTime.UtcNow >= deadline)
                {
                    throw new TimeoutException("Timed out waiting for parsed message to be queued.");
                }
                Thread.Sleep(1);
            }
            // Once the message is in the preprocessed queue, we can invoke Update() to handle events on the main thread.
            client.FrameTick();
        }

        client.Disconnect();

        // Verify dumped events and the final client state.
        events.Freeze();
        await Verify(
                new
                {
                    Events = events,
                    FinalSnapshot = new
                    {
                        User = client.Db.User.Iter().ToList(),
                        Message = client.Db.Message.Iter().ToList()
                    },
                    Stats = client.stats
                }
            )
            .UseParameters(dumpName)
            .AddExtraSettings(settings => settings.Converters.AddRange([
                new EventsConverter(),
                new TimestampConverter(),
                new EnergyQuantaConverter()
            ]))
            .ScrubMember<EventContext>(x => x.Db)
            .ScrubMember<EventContext>(x => x.Reducers);
    }
}
