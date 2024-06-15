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
public class IdentityConverter(Identity? myIdentity) : WriteOnlyJsonConverter<Identity>
{
    public override void Write(VerifyJsonWriter writer, Identity value)
    {
        if (value == myIdentity)
        {
            writer.WriteValue("(identity of A)");
        }
        else
        {
            writer.WriteValue("(identity of B)");
        }
    }
}

class AddressConverter : WriteOnlyJsonConverter<Address>
{
    public override void Write(VerifyJsonWriter writer, Address value)
    {
        writer.WriteValue(value.ToString());
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
                [new ByteStringConverter(), new AddressConverter()]
            )
        );

        VerifierSettings.IgnoreMember<ReducerEvent>(_ => _.ReducerName);
    }
}
