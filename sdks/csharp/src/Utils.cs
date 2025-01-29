using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Linq;
using SpacetimeDB.ClientApi;

#if !NET5_0_OR_GREATER
namespace System.Runtime.CompilerServices
{
    internal static class IsExternalInit { } // https://stackoverflow.com/a/64749403/1484415
}
#endif

namespace SpacetimeDB
{
    namespace ClientApi
    {
        public partial class BsatnRowList : IEnumerable<byte[]>
        {
            public IEnumerator<byte[]> GetEnumerator()
            {
                var rowsData = RowsData;

                var iter = SizeHint switch
                {
                    RowSizeHint.FixedSize(var size) => Enumerable
                        .Range(0, rowsData.Count / size)
                        .Select(index => rowsData.Skip(index * size).Take(size).ToArray()),

                    RowSizeHint.RowOffsets(var offsets) => offsets.Zip(
                        offsets.Skip(1).Append((ulong)rowsData.Count),
                        (start, end) => rowsData.Take((int)end).Skip((int)start).ToArray()
                    ),

                    _ => throw new InvalidOperationException("Unknown RowSizeHint variant"),
                };

                return iter.GetEnumerator();
            }

            IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
        }
    }
}
