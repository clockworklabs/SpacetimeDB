namespace SpacetimeDB.Tests;

using System.IO.Compression;
using System.Runtime.CompilerServices;
using Argon;
using Google.Protobuf;
using SpacetimeDB.Types;
using Xunit;

public class SnapshotTests
{
    class Events : List<KeyValuePair<string, object?>>
    {
        public void Add(string name, object? value = null)
        {
            base.Add(new(name, value));
        }
    }

    class TestLogger(Events events) : ISpacetimeDBLogger
    {
        public void Log(string message)
        {
            events.Add("Log", message);
        }

        public void LogWarning(string message)
        {
            events.Add("LogWarning", message);
        }

        public void LogError(string message)
        {
            events.Add("LogError", message);
        }

        public void LogException(Exception e)
        {
            events.Add("LogException", e.Message);
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

    class ByteStringReaderConverter : JsonConverter<ByteString>
    {
        public override ByteString ReadJson(
            JsonReader reader,
            Type type,
            ByteString? existingValue,
            bool hasExisting,
            JsonSerializer serializer
        )
        {
            var s = reader.StringValue;
            try
            {
                return ByteString.FromBase64(s);
            }
            catch
            {
                return ByteString.CopyFromUtf8(s);
            }
        }

        public override void WriteJson(
            JsonWriter writer,
            ByteString value,
            JsonSerializer serializer
        )
        {
            throw new NotImplementedException();
        }
    }

    private static string GetTestDir([CallerFilePath] string testFilePath = "") =>
        Path.GetDirectoryName(testFilePath)!;

    [Fact]
    public async Task VerifyAllTablesParsed()
    {
        var events = new Events();

        Logger.Current = new TestLogger(events);

        var client = SpacetimeDBClient.instance;

        var jsonSettings = new JsonSerializerSettings
        {
            Converters = [new ByteStringReaderConverter()]
        };

        // We store the dump in JSON-NL format for simplicity (it's just `ClientApi.Message.toString()`) and readability.
        var sampleDumpParsed = File.ReadLines(Path.Combine(GetTestDir(), "sample-dump.jsonl"))
            .Select(line => JsonConvert.DeserializeObject<ClientApi.Message>(line, jsonSettings));

        // But for proper testing we need to convert it back to raw binary messages as if it was received over network.
        var sampleDumpBinary = sampleDumpParsed.Select((message, i) =>
        {
            // Start tracking requests in the stats handler so that those request IDs can later be found.
            switch (message)
            {
                case {
                    TypeCase: ClientApi.Message.TypeOneofCase.SubscriptionUpdate,
                    SubscriptionUpdate: var subscriptionUpdate
                }:
                    client.stats.SubscriptionRequestTracker.StartTrackingRequest($"sample#{i}");
                    break;
                case {
                    TypeCase: ClientApi.Message.TypeOneofCase.TransactionUpdate,
                    TransactionUpdate: var transactionUpdate
                }:
                    client.stats.ReducerRequestTracker.StartTrackingRequest($"sample#{i}");
                    break;
            }
            using var output = new MemoryStream();
            using (var brotli = new BrotliStream(output, CompressionMode.Compress))
            {
                message.WriteTo(brotli);
            }
            return output.ToArray();
        });

        Identity? myIdentity = null;

        client.onBeforeSubscriptionApplied += () => events.Add("OnBeforeSubscriptionApplied");
        client.onEvent += (ev) => events.Add("OnEvent", ev);
        client.onIdentityReceived += (_authToken, identity, address) =>
        {
            myIdentity = identity;
            events.Add("OnIdentityReceived", new { identity, address });
        };
        client.onSubscriptionApplied += () => events.Add("OnSubscriptionApplied");
        client.onUnhandledReducerError += (exception) =>
            events.Add("OnUnhandledReducerError", exception);

        Reducer.OnSendMessageEvent += (reducerEvent, _text) =>
            events.Add("OnSendMessage", reducerEvent);
        Reducer.OnSetNameEvent += (reducerEvent, _name) => events.Add("OnSetName", reducerEvent);

        User.OnDelete += (user, reducerEvent) =>
            events.Add("OnDeleteUser", new { user, reducerEvent });
        User.OnInsert += (user, reducerEvent) =>
            events.Add("OnInsertUser", new { user, reducerEvent });
        User.OnUpdate += (oldUser, newUser, reducerEvent) =>
            events.Add(
                "OnUpdateUser",
                new
                {
                    oldUser,
                    newUser,
                    reducerEvent
                }
            );

        Message.OnDelete += (message, reducerEvent) =>
            events.Add("OnDeleteMessage", new { message, reducerEvent });
        Message.OnInsert += (message, reducerEvent) =>
            events.Add("OnInsertMessage", new { message, reducerEvent });

        // Simulate receiving WebSocket messages.
        foreach (var sample in sampleDumpBinary)
        {
            client.OnMessageReceived(sample, DateTime.UtcNow);
            // Wait for this message to be picked up by the background thread, preprocessed and stored in the preprocessed queue.
            // Otherwise we'll get inconsistent output order between test reruns.
            while (!client.HasPreProcessedMessage) { }
            // Once the message is in the preprocessed queue, we can invoke Update() to handle events on the main thread.
            client.Update();
        }

        // Verify dumped events and the final client state.
        await Verify(
                new
                {
                    Events = events,
                    FinalSnapshot = new
                    {
                        User = User.Iter().ToList(),
                        Message = Message.Iter().ToList()
                    },
                    Stats = client.stats
                }
            )
            .AddExtraSettings(settings =>
                settings.Converters.AddRange(
                    [
                        new EventsConverter(),
                        new IdentityConverter(myIdentity),
                        new NetworkRequestTrackerConverter()
                    ]
                )
            )
            .ScrubMember<ClientApi.Event>(_ => _.CallerIdentity);
    }
}
