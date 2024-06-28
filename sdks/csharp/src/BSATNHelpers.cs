using System;
using SpacetimeDB.BSATN;
using System.IO;
using SpacetimeDB.ClientApi;

namespace SpacetimeDB
{
    public static class BSATNHelpers {
        public static T FromStream<T>(Stream stream)
            where T : IStructuralReadWrite, new()
        {
            using var reader = new BinaryReader(stream);
            return IStructuralReadWrite.Read<T>(reader);
        }

        public static T Decode<T>(byte[] bsatn) where T : IStructuralReadWrite, new() {
            using var stream = new MemoryStream(bsatn);
            return FromStream<T>(stream);
        }

        public static T Decode<T>(string json)
            where T : IStructuralReadWrite, new()
        {
            throw new InvalidOperationException("JSON isn't supported at the moment");
        }

        public static T Decode<T>(EncodedValue value) where T : IStructuralReadWrite, new() => value switch
        {
            EncodedValue.Binary(var bin) => Decode<T>(bin),
            EncodedValue.Text(var text) => Decode<T>(text),
            _ => throw new InvalidOperationException()
        };
    }
}
