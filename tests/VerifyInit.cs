namespace SpacetimeDB.Tests;

using System.Runtime.CompilerServices;
using Google.Protobuf;
using SpacetimeDB.Types;

public static class VerifyInit
{
    class ByteStringConverter : WriteOnlyJsonConverter<ByteString>
    {
        public override void Write(VerifyJsonWriter writer, ByteString value)
        {
            writer.WriteValue(Convert.ToHexString(value.Span));
        }
    }

    class IdentityOrAddressConverter : WriteOnlyJsonConverter
    {
        public override bool CanConvert(Type objectType)
        {
            objectType = Nullable.GetUnderlyingType(objectType) ?? objectType;
            return objectType == typeof(Identity) || objectType == typeof(Address);
        }

        public override void Write(VerifyJsonWriter writer, object value)
        {
            writer.WriteValue(value.ToString());
        }
    }

    [ModuleInitializer]
    public static void Init()
    {
        Environment.SetEnvironmentVariable("DiffEngine_TargetOnLeft", "true");

        VerifierSettings.AddExtraSettings(settings =>
            settings.Converters.AddRange(
                [new ByteStringConverter(), new IdentityOrAddressConverter()]
            )
        );

        VerifierSettings.IgnoreMember<ReducerEvent>(_ => _.ReducerName);
    }
}
