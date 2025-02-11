namespace SpacetimeDB.Tests;

using System.IO.Compression;
using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;
using SpacetimeDB.Types;
using Xunit;

using U128 = SpacetimeDB.U128;

public class SnapshotTests
{
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

    private static ServerMessage.IdentityToken SampleId(string identity, string token, string address) =>
        new(new()
        {
            Identity = Identity.From(Convert.FromBase64String(identity)),
            Token = token,
            ConnectionId = ConnectionId.From(Convert.FromBase64String(address)) ?? throw new InvalidDataException("address")
        });

    private static ServerMessage.InitialSubscription SampleLegacyInitialSubscription(
        uint requestId,
        long hostExecutionDuration,
        List<TableUpdate> updates
    ) => new(new()
    {
        RequestId = requestId,
        TotalHostExecutionDuration = new TimeDuration(hostExecutionDuration),
        DatabaseUpdate = new DatabaseUpdate
        {
            Tables = updates
        }
    });

    private static ServerMessage.SubscribeApplied SampleSubscribeApplied(
        uint requestId,
        uint queryId,
        ulong hostExecutionDuration,
        TableUpdate tableUpdate
    ) => new(new()
    {
        RequestId = requestId,
        TotalHostExecutionDurationMicros = hostExecutionDuration,
        QueryId = new(queryId),
        Rows = new()
        {
            // This message contains redundant data, shrug.
            // Copy out the redundant fields.
            TableId = tableUpdate.TableId,
            TableName = tableUpdate.TableName,
            TableRows = tableUpdate
        }
    });

    private static ServerMessage.UnsubscribeApplied SampleUnsubscribeApplied(
        uint requestId,
        uint queryId,
        ulong hostExecutionDuration,
        TableUpdate tableUpdate
    ) => new(new()
    {
        RequestId = requestId,
        TotalHostExecutionDurationMicros = hostExecutionDuration,
        QueryId = new(queryId),
        Rows = new()
        {
            // This message contains redundant data, shrug.
            // Copy out the redundant fields.
            TableId = tableUpdate.TableId,
            TableName = tableUpdate.TableName,
            TableRows = tableUpdate
        }
    });

    private static ServerMessage.SubscriptionError SampleSubscriptionError(
        uint? requestId,
        uint? queryId,
        uint? tableId,
        string error,
        ulong hostExecutionDuration
    ) => new(new()
    {
        RequestId = requestId,
        QueryId = queryId,
        TableId = tableId,
        Error = error,
        TotalHostExecutionDurationMicros = hostExecutionDuration,
    });

    private static ServerMessage.TransactionUpdate SampleTransactionUpdate(
        long microsecondsSinceUnixEpoch,
        string callerIdentity,
        string callerConnectionId,
        uint requestId,
        string reducerName,
        ulong energyQuantaUsed,
        long hostExecutionDurationMicros,
        List<TableUpdate> updates,
        byte[]? args
    ) => new(new()
    {
        Timestamp = new Timestamp { MicrosecondsSinceUnixEpoch = microsecondsSinceUnixEpoch },
        CallerIdentity = Identity.From(Convert.FromBase64String(callerIdentity)),
        CallerConnectionId = ConnectionId.From(Convert.FromBase64String(callerConnectionId)) ?? throw new InvalidDataException("callerConnectionId"),
        TotalHostExecutionDuration = new TimeDuration(hostExecutionDurationMicros),
        EnergyQuantaUsed = new()
        {
            Quanta = new U128(0, energyQuantaUsed),
        },
        ReducerCall = new()
        {
            RequestId = requestId,
            ReducerName = reducerName,
            Args = [.. (args ?? [])]
        },
        Status = new UpdateStatus.Committed(new()
        {
            Tables = updates
        })
    });

    private static TableUpdate SampleUpdate<T>(
        uint tableId,
        string tableName,
        List<T> inserts,
        List<T> deletes
    ) where T : IStructuralReadWrite => new()
    {
        TableId = tableId,
        TableName = tableName,
        NumRows = (ulong)(inserts.Count + deletes.Count),
        Updates = [new CompressableQueryUpdate.Uncompressed(new QueryUpdate(
            EncodeRowList<T>(deletes), EncodeRowList<T>(inserts)))]
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

    private static readonly uint USER_TABLE_ID = 4097;
    private static readonly string USER_TABLE_NAME = "user";
    private static readonly uint MESSAGE_TABLE_ID = 4098;
    private static readonly string MESSAGE_TABLE_NAME = "message";


    private static TableUpdate SampleUserInsert(string identity, string? name, bool online) =>
        SampleUpdate(USER_TABLE_ID, USER_TABLE_NAME, [new User
        {
            Identity = Identity.From(Convert.FromBase64String(identity)),
            Name = name,
            Online = online
        }], []);

    private static TableUpdate SampleUserUpdate(string identity, string? oldName, string? newName, bool oldOnline, bool newOnline) =>
        SampleUpdate(USER_TABLE_ID, USER_TABLE_NAME, [new User
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
        SampleUpdate(MESSAGE_TABLE_ID, MESSAGE_TABLE_NAME, messages, []);

    private static TableUpdate SampleMessageDelete(List<Message> messages) =>
        SampleUpdate(MESSAGE_TABLE_ID, MESSAGE_TABLE_NAME, [], messages);

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
            SampleLegacyInitialSubscription(
                1, 366, [SampleUserInsert("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, true)]
            ),
            SampleTransactionUpdate(0, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
                0, "unknown-reducer", 0, 40, [
SampleUserInsert("k5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, true)
                ], null
            ),
            SampleTransactionUpdate(
                1718487763059031, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
                0, "identity_connected", 1957615, 66, [SampleUserInsert("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", null, true)],
                null
            ),
            SampleTransactionUpdate(
                1718487768057579, "j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", "Vd4dFzcEzhLHJ6uNL8VXFg==",
                1, "set_name", 4345615, 70, [SampleUserUpdate("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, "A", true, true)],
                Encode(new Reducer.SetName { Name = "A" })
            ),
            SampleTransactionUpdate(
                1718487775346381, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
                1, "send_message", 2779615, 57, [SampleMessageInsert([
                    sampleMessage0
                ])],
                Encode(new Reducer.SendMessage { Text = "Hello, A!" })
            ),
            SampleTransactionUpdate(
                1718487777307855, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
                2, "set_name", 4268615, 98, [SampleUserUpdate("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", null, "B", true, true)],
                Encode(new Reducer.SetName { Name = "B" })
            ),
            SampleTransactionUpdate(
                1718487783175083, "j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", "Vd4dFzcEzhLHJ6uNL8VXFg==",
                2, "send_message", 2677615, 40, [SampleMessageInsert([
                    sampleMessage1
                ])],
                Encode(new Reducer.SendMessage { Text = "Hello, B!" })
            ),
            SampleTransactionUpdate(
                1718487787645364, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
                3, "send_message", 2636615, 28, [SampleMessageInsert([
                    sampleMessage2
                ])],
                Encode(new Reducer.SendMessage { Text = "Goodbye!" })
            ),
            SampleTransactionUpdate(
                1718487791901504, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
                0, "identity_disconnected", 3595615, 75, [SampleUserUpdate("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "B", "B", true, false)],
                null
            ),
            SampleTransactionUpdate(
                1718487794937841, "j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", "Vd4dFzcEzhLHJ6uNL8VXFg==",
                3, "send_message", 2636615, 34, [SampleMessageInsert([
                    sampleMessage3
                ])],
                Encode(new Reducer.SendMessage { Text = "Goodbye!" })
            ),
            }
        };
        yield return new object[] { "SubscribeApplied",
            new ServerMessage[] {
            SampleId(
                "j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=",
                "eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9.eyJoZXhfaWRlbnRpdHkiOiJjMjAwNDgzMTUyZDY0MmM3ZDQwMmRlMDZjYWNjMzZkY2IwYzJhMWYyYmJlYjhlN2Q1YTY3M2YyNDM1Y2NhOTc1Iiwic3ViIjoiNmQ0YjU0MzAtMDBjZi00YTk5LTkzMmMtYWQyZDA3YmFiODQxIiwiaXNzIjoibG9jYWxob3N0IiwiYXVkIjpbInNwYWNldGltZWRiIl0sImlhdCI6MTczNzY2NTc2OSwiZXhwIjpudWxsfQ.GaKhvswWYW6wpPpK70_-Tw8DKjKJ2qnidwwj1fTUf3mctcsm_UusPYSws_pSW3qGnMNnGjEXt7rRNvGvuWf9ow",
                "Vd4dFzcEzhLHJ6uNL8VXFg=="
            ),
            SampleSubscribeApplied(
                1, 1, 366, SampleUserInsert("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, true)
            ),
            SampleSubscribeApplied(
                1, 2, 277, SampleUpdate<Message>(MESSAGE_TABLE_ID, MESSAGE_TABLE_NAME, [], [])
            ),
            SampleTransactionUpdate(0, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
                0, "unknown-reducer", 0, 40, [
                    SampleUserInsert("k5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, true)
                ], null
            ),
            SampleTransactionUpdate(
                1718487763059031, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
                0, "identity_connected", 1957615, 66, [SampleUserInsert("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", null, true)],
                null
            ),
            SampleTransactionUpdate(
                1718487768057579, "j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", "Vd4dFzcEzhLHJ6uNL8VXFg==",
                1, "set_name", 4345615, 70, [SampleUserUpdate("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, "A", true, true)],
                Encode(new Reducer.SetName { Name = "A" })
            ),
            SampleTransactionUpdate(
                1718487775346381, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
                1, "send_message", 2779615, 57, [SampleMessageInsert([
                    sampleMessage0
                ])],
                Encode(new Reducer.SendMessage { Text = "Hello, A!" })
            ),
            SampleTransactionUpdate(
                1718487777307855, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
                2, "set_name", 4268615, 98, [SampleUserUpdate("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", null, "B", true, true)],
                Encode(new Reducer.SetName { Name = "B" })
            ),
            SampleTransactionUpdate(
                1718487783175083, "j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", "Vd4dFzcEzhLHJ6uNL8VXFg==",
                2, "send_message", 2677615, 40, [SampleMessageInsert([
                    sampleMessage1
                ])],
                Encode(new Reducer.SendMessage { Text = "Hello, B!" })
            ),
            SampleTransactionUpdate(
                1718487787645364, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
                3, "send_message", 2636615, 28, [SampleMessageInsert([
                    sampleMessage2
                ])],
                Encode(new Reducer.SendMessage { Text = "Goodbye!" })
            ),
            SampleTransactionUpdate(
                1718487791901504, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
                0, "identity_disconnected", 3595615, 75, [SampleUserUpdate("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "B", "B", true, false)],
                null
            ),
            SampleTransactionUpdate(
                1718487794937841, "j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", "Vd4dFzcEzhLHJ6uNL8VXFg==",
                3, "send_message", 2636615, 34, [SampleMessageInsert([
                    sampleMessage3
                ])],
                Encode(new Reducer.SendMessage { Text = "Goodbye!" })
            ),
            // Let's pretend the user unsubscribed from the table Messages...
            SampleUnsubscribeApplied(0,
            2, 55, SampleMessageDelete([sampleMessage0, sampleMessage1, sampleMessage2, sampleMessage3])),
            // Tried to resubscribe unsuccessfully...
            SampleSubscriptionError(0, 3, MESSAGE_TABLE_ID, "bad query dude", 69),
            // Then successfully resubscribed.
            SampleSubscribeApplied(0, 4, 53, SampleMessageInsert([sampleMessage0, sampleMessage1, sampleMessage2, sampleMessage3]))
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
            .WithModuleName("example")
            .OnConnect((conn, identity, token) => events.Add("OnConnect", new { identity, token }))
            .Build();

        // But for proper testing we need to convert it back to raw binary messages as if it was received over network.
        var sampleDumpBinary = sampleDumpParsed.Select(
            (message, i) =>
            {
                // Start tracking requests in the stats handler so that those request IDs can later be found.
                switch (message)
                {
                    case ServerMessage.InitialSubscription(var _):
                        client.stats.SubscriptionRequestTracker.StartTrackingRequest($"sample#{i}");
                        break;
                    case ServerMessage.SubscribeApplied(var _):
                        client.stats.SubscriptionRequestTracker.StartTrackingRequest($"sample#{i}");
                        break;
                    case ServerMessage.TransactionUpdate(var _):
                        client.stats.ReducerRequestTracker.StartTrackingRequest($"sample#{i}");
                        break;
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

        client.Reducers.OnSendMessage += (eventContext, _text) =>
            events.Add("OnSendMessage", eventContext);
        client.Reducers.OnSetName += (eventContext, _name) => events.Add("OnSetName", eventContext);

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

        // Simulate receiving WebSocket messages.
        foreach (var sample in sampleDumpBinary)
        {
            client.OnMessageReceived(sample, DateTime.UtcNow);
            // Wait for this message to be picked up by the background thread, preprocessed and stored in the preprocessed queue.
            // Otherwise we'll get inconsistent output order between test reruns.
            while (!client.HasPreProcessedMessage) { }
            // Once the message is in the preprocessed queue, we can invoke Update() to handle events on the main thread.
            client.FrameTick();
        }

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
            .ScrubMember<TransactionUpdate>(x => x.Status)
            .ScrubMember<EventContext>(x => x.Db)
            .ScrubMember<EventContext>(x => x.Reducers);
    }
}
