using SpacetimeDB.BSATN;
using System.IO;

namespace SpacetimeDB
{
    public static class BSATNHelpers
    {
        public static T Decode<T>(System.Collections.Generic.List<byte> bsatn) where T : IStructuralReadWrite, new() =>
            Decode<T>(bsatn.ToArray());

        public static T Decode<T>(byte[] bsatn) where T : IStructuralReadWrite, new()
        {
            using var stream = new MemoryStream(bsatn);
            using var reader = new BinaryReader(stream);
            return IStructuralReadWrite.Read<T>(reader);
        }
    }
}
