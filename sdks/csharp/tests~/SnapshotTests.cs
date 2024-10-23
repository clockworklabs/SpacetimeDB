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
            writer.WriteValue(timestamp.Microseconds);
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

    private static IdentityToken SampleId(string identity, string token, string address) =>
        new()
        {
            Identity = Identity.From(Convert.FromBase64String(identity)),
            Token = token,
            Address = Address.From(Convert.FromBase64String(address)) ?? throw new InvalidDataException("address")
        };

    private static ServerMessage.AfterConnecting SampleHandshake(string identity, string token, string address, IdsToNames idsToNames) =>
        new(new(SampleId(identity, token, address), idsToNames));

    private static ServerMessage.InitialSubscription SampleSubscriptionUpdate(
        uint requestId,
        ulong hostExecutionDuration,
        List<TableUpdate> updates
    ) => new(new()
    {
        RequestId = requestId,
        TotalHostExecutionDurationMicros = hostExecutionDuration,
        DatabaseUpdate = new DatabaseUpdate
        {
            Tables = updates
        }
    });

    private static ServerMessage.TransactionUpdate SampleTransactionUpdate(
        ulong timestamp,
        string callerIdentity,
        string callerAddress,
        uint requestId,
        uint reducerId,
        ulong energyQuantaUsed,
        ulong hostExecutionDuration,
        List<TableUpdate> updates,
        byte[]? args
    ) => new(new()
    {
        Timestamp = new Timestamp { Microseconds = timestamp },
        CallerIdentity = Identity.From(Convert.FromBase64String(callerIdentity)),
        CallerAddress = Address.From(Convert.FromBase64String(callerAddress)) ?? throw new InvalidDataException("callerAddress"),
        HostExecutionDurationMicros = hostExecutionDuration,
        EnergyQuantaUsed = new()
        {
            Quanta = new U128(0, energyQuantaUsed),
        },
        ReducerCall = new()
        {
            RequestId = requestId,
            ReducerId = reducerId,
            Args = args ?? []
        },
        Status = new UpdateStatus.Committed(new()
        {
            Tables = updates
        })
    });

    private static TableUpdate SampleUpdate<T>(
        uint tableId,
        List<T> inserts,
        List<T> deletes
    ) where T : IStructuralReadWrite => new()
    {
        TableId = tableId,
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
            RowsData = stream.ToArray(),
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

    private static TableUpdate SampleUserInsert(string identity, string? name, bool online) =>
        SampleUpdate(4097, [new User
        {
            Identity = Identity.From(Convert.FromBase64String(identity)),
            Name = name,
            Online = online
        }], []);

    private static TableUpdate SampleUserUpdate(string identity, string? oldName, string? newName, bool oldOnline, bool newOnline) =>
        SampleUpdate(4097, [new User
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

    private static TableUpdate SampleMessage(string identity, ulong sent, string text) =>
        SampleUpdate(4098, [new Message
        {
            Sender = Identity.From(Convert.FromBase64String(identity)),
            Sent = sent,
            Text = text
        }], []);

    private static ServerMessage[] SampleDump() => [
        SampleHandshake(
            "j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=",
            "eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9.eyJoZXhfaWRlbnRpdHkiOiI4ZjkwY2M5NGE5OTY4ZGY2ZDI5N2JhYTY2NTAzYTg5M2IxYzM0YjBiMDAyNjhhNTE0ODk4ZGQ5NTRiMGRhMjBiIiwiaWF0IjoxNzE4NDg3NjY4LCJleHAiOm51bGx9.PSn481bLRqtFwIh46nOXDY14X3GKbz8t4K4GmBmz50loU6xzeL7zDdCh1V2cmiQsoGq8Erxg0r_6b6Y5SqKoBA",
            "Vd4dFzcEzhLHJ6uNL8VXFg==",
            new IdsToNames(
                [],
                [],
                [4097, 4098],
                ["user", "message"]
            )
        ),
        SampleSubscriptionUpdate(
            1, 366, [SampleUserInsert("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, true)]
        ),
        SampleTransactionUpdate(0, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
            0, 50, 0, 40, [], null
        ),
        SampleTransactionUpdate(
            1718487763059031, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
            0, 0, 1957615, 66, [SampleUserInsert("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", null, true)],
            null
        ),
        SampleTransactionUpdate(
            1718487768057579, "j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", "Vd4dFzcEzhLHJ6uNL8VXFg==",
            1, 4, 4345615, 70, [SampleUserUpdate("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", null, "A", true, true)],
            Encode(new SetName { Name = "A" })
        ),
        SampleTransactionUpdate(
            1718487775346381, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
            1, 3, 2779615, 57, [SampleMessage("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", 1718487775346381, "Hello, A!")],
            Encode(new SendMessage { Text = "Hello, A!" })
        ),
        SampleTransactionUpdate(
            1718487777307855, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
            2, 4, 4268615, 98, [SampleUserUpdate("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", null, "B", true, true)],
            Encode(new SetName { Name = "B" })
        ),
        SampleTransactionUpdate(
            1718487783175083, "j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", "Vd4dFzcEzhLHJ6uNL8VXFg==",
            2, 3, 2677615, 40, [SampleMessage("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", 1718487783175083, "Hello, B!")],
            Encode(new SendMessage { Text = "Hello, B!" })
        ),
        SampleTransactionUpdate(
            1718487787645364, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
            3, 3, 2636615, 28, [SampleMessage("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", 1718487787645364, "Goodbye!")],
            Encode(new SendMessage { Text = "Goodbye!" })
        ),
        SampleTransactionUpdate(
            1718487791901504, "l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "Kwmeu5riP20rvCTNbBipLA==",
            0, 1, 3595615, 75, [SampleUserUpdate("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY=", "B", "B", true, false)],
            null
        ),
        SampleTransactionUpdate(
            1718487794937841, "j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", "Vd4dFzcEzhLHJ6uNL8VXFg==",
            3, 3, 2636615, 34, [SampleMessage("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs=", 1718487794937841, "Goodbye!")],
            Encode(new SendMessage { Text = "Goodbye!" })
        ),
    ];

    [Fact]
    public async Task VerifyAllTablesParsed()
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

        var sampleDumpParsed = SampleDump();

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

#pragma warning disable CS0612 // Using obsolete API
        client.onUnhandledReducerError += (exception) =>
            events.Add("OnUnhandledReducerError", exception);
#pragma warning restore CS0612 // Using obsolete API
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
            .AddExtraSettings(settings => settings.Converters.AddRange([
                new EventsConverter(),
                new TimestampConverter(),
                new EnergyQuantaConverter()
            ]))
            .ScrubMember<TransactionUpdate>(x => x.Status)
            .ScrubMember<DbContext<RemoteTables>>(x => x.Db)
            .ScrubMember<EventContext>(x => x.Reducers);
    }
}
