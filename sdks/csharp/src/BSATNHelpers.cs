using SpacetimeDB.BSATN;
using System.IO;

namespace SpacetimeDB
{
    public static class BSATNHelpers
    {
        public static T Decode<T>(byte[] bsatn) where T : IStructuralReadWrite, new()
        {
            using var stream = new MemoryStream(bsatn);
            using var reader = new BinaryReader(stream);
            return IStructuralReadWrite.Read<T>(reader);
        }
    }
}
