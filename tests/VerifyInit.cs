namespace SpacetimeDB.Tests;

using System.Runtime.CompilerServices;
using Google.Protobuf;
using SpacetimeDB.Types;

class ByteStringConverter : WriteOnlyJsonConverter<ByteString>
{
    public override void Write(VerifyJsonWriter writer, ByteString value)
    {
        writer.WriteValue(Convert.ToHexString(value.Span));
    }
}

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

class AddressConverter : WriteOnlyJsonConverter<Address>
{
    public override void Write(VerifyJsonWriter writer, Address value)
    {
        // Addresses are GUIDs, which Verify scrubs automatically.
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
            writer.WriteMember(value, requestsAwaitingResponse, nameof(requestsAwaitingResponse));
        }

        if (value.GetMinMaxTimes(int.MaxValue) is { Min.Metadata: var Min, Max.Metadata: var Max })
        {
            writer.WriteMember(value, Min, nameof(Min));
            writer.WriteMember(value, Max, nameof(Max));
        }

        writer.WriteEndObject();
    }
}

static class VerifyInit
{
    [ModuleInitializer]
    public static void Init()
    {
        Environment.SetEnvironmentVariable("DiffEngine_TargetOnLeft", "true");

        VerifierSettings.AddExtraSettings(settings =>
            settings.Converters.AddRange(
                [new ByteStringConverter(), new IdentityConverter(), new AddressConverter()]
            )
        );

        VerifierSettings.IgnoreMember<ReducerEvent>(_ => _.ReducerName);
    }
}
