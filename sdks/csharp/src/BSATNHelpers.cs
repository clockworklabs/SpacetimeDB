using SpacetimeDB.BSATN;
using System.Collections.Generic;
using System.IO;

namespace SpacetimeDB
{
    public static class BSATNHelpers
    {
        /// <summary>
        /// Decode an element of a BSATN-serializable type from a list of bytes.
        ///
        /// This method performs several allocations. Prefer calling <c>IStructuralReadWrite.Read<T>(BinaryReader)</c> when
        /// deserializing many items from a buffer.
        /// </summary>
        /// <typeparam name="T"></typeparam>
        /// <param name="bsatn"></param>
        /// <returns></returns>
        public static T Decode<T>(System.Collections.Generic.List<byte> bsatn) where T : IStructuralReadWrite, new() =>
            Decode<T>((IReadOnlyList<byte>)bsatn);

        /// <summary>
        /// Decode an element of a BSATN-serializable type from a readonly byte list.
        /// </summary>
        /// <typeparam name="T"></typeparam>
        /// <param name="bsatn"></param>
        /// <returns></returns>
        public static T Decode<T>(IReadOnlyList<byte> bsatn) where T : IStructuralReadWrite, new()
        {
            using Stream stream = bsatn is byte[] bytes
                ? new MemoryStream(bytes)
                : new ListStream(bsatn);
            using var reader = new BinaryReader(stream);
            return IStructuralReadWrite.Read<T>(reader);
        }

        /// <summary>
        /// Decode an element of a BSATN-serializable type from a byte array.
        ///
        /// This method performs several allocations. Prefer calling <c>IStructuralReadWrite.Read<T>(BinaryReader)</c> when
        /// deserializing many items from a buffer.
        /// </summary>
        /// <typeparam name="T"></typeparam>
        /// <param name="bsatn"></param>
        /// <returns></returns>
        public static T Decode<T>(byte[] bsatn) where T : IStructuralReadWrite, new()
        {
            using var stream = new MemoryStream(bsatn);
            using var reader = new BinaryReader(stream);
            return IStructuralReadWrite.Read<T>(reader);
        }
    }
}
