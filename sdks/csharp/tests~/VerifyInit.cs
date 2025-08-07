namespace SpacetimeDB.Tests;

using System.Runtime.CompilerServices;
using Argon;
using SpacetimeDB.Types;

static class VerifyInit
{
    // A converter that scrubs identity to a stable string.
    class IdentityConverter : WriteOnlyJsonConverter<Identity>
    {
        private static readonly List<Identity> seenIdentities = [];

        public override void Write(VerifyJsonWriter writer, Identity value)
        {
            var index = seenIdentities.IndexOf(value);
            if (index == -1)
            {
                index = seenIdentities.Count;
                seenIdentities.Add(value);
            }

            writer.WriteValue($"Identity_{index + 1}");
        }
    }

    class ConnectionIdConverter : WriteOnlyJsonConverter<ConnectionId>
    {
        public override void Write(VerifyJsonWriter writer, ConnectionId value)
        {
            // ConnectionIdes are GUIDs, which Verify scrubs automatically.
            writer.WriteValue(value.ToString());
        }
    }

    class NetworkRequestTrackerConverter : WriteOnlyJsonConverter<NetworkRequestTracker>
    {
        public override void Write(VerifyJsonWriter writer, NetworkRequestTracker value)
        {
            writer.WriteStartObject();

            var sampleCount = value.GetSampleCount();
            if (sampleCount > 0)
            {
                writer.WriteMember(value, sampleCount, nameof(sampleCount));
            }

            var requestsAwaitingResponse = value.GetRequestsAwaitingResponse();
            if (requestsAwaitingResponse > 0)
            {
                writer.WriteMember(
                    value,
                    requestsAwaitingResponse,
                    nameof(requestsAwaitingResponse)
                );
            }

            // We don't use the stats, since they are nondeterministic.

            writer.WriteEndObject();
        }
    }

    [ModuleInitializer]
    public static void Init()
    {
        Environment.SetEnvironmentVariable("DiffEngine_TargetOnLeft", "true");

        VerifierSettings.AddExtraSettings(settings =>
        {
            settings.Converters.AddRange(
                [
                    new IdentityConverter(),
                    new ConnectionIdConverter(),
                    new NetworkRequestTrackerConverter()
                ]
            );
            settings.TypeNameHandling = TypeNameHandling.Auto;
        });
    }
}
