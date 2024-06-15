namespace SpacetimeDB.Tests;

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
            // We don't start tracking in test simulation, so those warnings are expected.
            if (message.StartsWith("Failed to finish tracking"))
            {
                return;
            }
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

    [Fact]
    public async Task VerifyAllTablesParsed()
    {
        // A dump of sample raw WebSocket messages.
        var wsSamples = new[]
        {
            "i6SAKscCCiCXSrMbUY9G0LWbCv7nir3y2/TfbmCjMtzpw3Ori+vOphKQAmV5SjBlWEFpT2lKS1YxUWlMQ0poYkdjaU9pSkZVekkxTmlKOS5leUpvWlhoZmFXUmxiblJwZEhraU9pSTVOelJoWWpNeFlqVXhPR1kwTm1Rd1lqVTVZakJoWm1WbE56aGhZbVJtTW1SaVpqUmtaalpsTmpCaE16TXlaR05sT1dNek56TmhZamhpWldKalpXRTJJaXdpYVdGMElqb3hOekUzTlRJek1qTXhMQ0psZUhBaU9tNTFiR3g5LjNRZEpkc2tLdTRlamdZMXVvQmk3TTBQN1QyaXpaSEhITXhDcEpTRWROU1dEaXZzZ0FFdkZJUmZxVUFKY0J0U0txaEZPaXRiZHhIUVJKd3VaQV8wejdRGhBgfJ3DgQmutLQUG2NU/BhfAw==",
            "ix6AEjwKNQiBIBIEVXNlchoqCAEaJiAAAACXSrMbUY9G0LWbCv7nir3y2/TfbmCjMtzpw3Ori+vOpgEBEAEYuwgD",
            "i2WAIskBCloIzIrmlIPehgMSIJdKsxtRj0bQtZsK/ueKvfLb9N9uYKMy3OnDc6uL686mGhMKCHNldF9uYW1lEgUBAAAAQRgBMI+eiQI4kgFCEGB8ncOBCa60tBQbY1T8GF8SawpkCIEgEgRVc2VyGigaJiAAAACXSrMbUY9G0LWbCv7nir3y2/TfbmCjMtzpw3Ori+vOpgEBGi8IARorIAAAAJdKsxtRj0bQtZsK/ueKvfLb9N9uYKMy3OnDc6uL686mAAEAAABBARABGLsQAw==",
            "i2KAIsMBCmkI99mQmYPehgMSIJdKsxtRj0bQtZsK/ueKvfLb9N9uYKMy3OnDc6uL686mGiMKDHNlbmRfbWVzc2FnZRIRDQAAAEhlbGxvLCB3b3JsZCEYAjDfsrQBOCpCEGB8ncOBCa60tBQbY1T8GF8SVgpPCIIgEgdNZXNzYWdlGkEIARo9IAAAAJdKsxtRj0bQtZsK/ueKvfLb9N9uYKMy3OnDc6uL686m9ywkM/AaBgANAAAASGVsbG8sIHdvcmxkIRACGP8HAw==",
            "C02AIpgBCl0I5obruIPehgMSIKtdmHT9AV3sDUOODydYrtVLR90/0mJ5TLJCwBmnpDdEGhgKFl9faWRlbnRpdHlfY29ubmVjdGVkX18w7713OEZCEJC8PImObEANVUiHlRorK9kSNwo1CIEgEgRVc2VyGioIARomIAAAAKtdmHT9AV3sDUOODydYrtVLR90/0mJ5TLJCwBmnpDdEAQED",
            "i6SAKscCCiCrXZh0/QFd7A1Djg8nWK7VS0fdP9JieUyyQsAZp6Q3RBKQAmV5SjBlWEFpT2lKS1YxUWlMQ0poYkdjaU9pSkZVekkxTmlKOS5leUpvWlhoZmFXUmxiblJwZEhraU9pSmhZalZrT1RnM05HWmtNREUxWkdWak1HUTBNemhsTUdZeU56VTRZV1ZrTlRSaU5EZGtaRE5tWkRJMk1qYzVOR05pTWpReVl6QXhPV0UzWVRRek56UTBJaXdpYVdGMElqb3hOekU0TkRZNE9EYzVMQ0psZUhBaU9tNTFiR3g5LjNFWXQ1NkRpc2FBOUZfV0NOemhMSDU1LWh4TldNNEhQYjdQQi03OWx3Y2Ffcy1YSllBaXB2ZDlONmRXY2xYb3VmU0JyeXhuQlBkVTVsMHRoZHpoMmhBGhCQvDyJjmxADVVIh5UaKyvZAw==",
            "C2CAEr4BCmYIgSASBFVzZXIaLwgBGisgAAAAl0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqYAAQAAAEEBGioIARomIAAAAKtdmHT9AV3sDUOODydYrtVLR90/0mJ5TLJCwBmnpDdEAQEKTwiCIBIHTWVzc2FnZRpBCAEaPSAAAACXSrMbUY9G0LWbCv7nir3y2/TfbmCjMtzpw3Ori+vOpvcsJDPwGgYADQAAAEhlbGxvLCB3b3JsZCEQARipAgM=",
            "C2WAIsgBClkIw8m6uoPehgMSIKtdmHT9AV3sDUOODydYrtVLR90/0mJ5TLJCwBmnpDdEGhMKCHNldF9uYW1lEgUBAAAAQhgBMMfEhAI4TUIQkLw8iY5sQA1VSIeVGisr2RJrCmQIgSASBFVzZXIaKBomIAAAAKtdmHT9AV3sDUOODydYrtVLR90/0mJ5TLJCwBmnpDdEAQEaLwgBGisgAAAAq12YdP0BXewNQ44PJ1iu1UtH3T/SYnlMskLAGaekN0QAAQAAAEIBEAEYqgYD",
            "C2WAIsgBClkIw8m6uoPehgMSIKtdmHT9AV3sDUOODydYrtVLR90/0mJ5TLJCwBmnpDdEGhMKCHNldF9uYW1lEgUBAAAAQhgBMMfEhAI4TUIQkLw8iY5sQA1VSIeVGisr2RJrCmQIgSASBFVzZXIaKBomIAAAAKtdmHT9AV3sDUOODydYrtVLR90/0mJ5TLJCwBmnpDdEAQEaLwgBGisgAAAAq12YdP0BXewNQ44PJ1iu1UtH3T/SYnlMskLAGaekN0QAAQAAAEIBEAEYqwYD",
            "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",
        };

        var events = new Events();

        Logger.Current = new TestLogger(events);

        var client = SpacetimeDBClient.instance;

        client.onBeforeSubscriptionApplied += () => events.Add("OnBeforeSubscriptionApplied");
        client.onEvent += (ev) => events.Add("OnEvent", ev);
        client.onIdentityReceived += (_authToken, identity, address) =>
            events.Add("OnIdentityReceived", new { identity, address });
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
        foreach (var wsSample in wsSamples)
        {
            client.OnMessageReceived(Convert.FromBase64String(wsSample), DateTime.UtcNow);
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
                    }
                }
            )
            .AddExtraSettings(settings => settings.Converters.Add(new EventsConverter()))
            .ScrubLinesWithReplace(s =>
                s.Replace(
                        "974AB31B518F46D0B59B0AFEE78ABDF2DBF4DF6E60A332DCE9C373AB8BEBCEA6",
                        "(identity of A)"
                    )
                    .Replace(
                        "AB5D9874FD015DEC0D438E0F2758AED54B47DD3FD262794CB242C019A7A43744",
                        "(identity of B)"
                    )
            );
    }
}
