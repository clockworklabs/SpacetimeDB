using System;
using System.IO;
using System.IO.Compression;
using SpacetimeDB.ClientApi;

namespace SpacetimeDB
{
    internal class CompressionHelpers
    {
        internal enum CompressionAlgos : byte
        {
            None = 0,
            Brotli = 1,
            Gzip = 2,
        }

        /// <summary>
        /// Creates a <see cref="BrotliStream"/> for decompressing the provided stream.
        /// </summary>
        /// <param name="stream">The input stream containing Brotli-compressed data.</param>
        /// <returns>A <see cref="BrotliStream"/> set to decompression mode.</returns>
        internal static BrotliStream BrotliReader(Stream stream)
        {
            return new BrotliStream(stream, CompressionMode.Decompress);
        }

        /// <summary>
        /// Creates a <see cref="GZipStream"/> for decompressing the provided stream.
        /// </summary>
        /// <param name="stream">The input stream containing GZip-compressed data.</param>
        /// <returns>A <see cref="GZipStream"/> set to decompression mode.</returns>
        internal static GZipStream GzipReader(Stream stream)
        {
            return new GZipStream(stream, CompressionMode.Decompress);
        }

        /// <summary>
        /// Decompresses and decodes a serialized <see cref="ServerMessage"/> from a byte array,
        /// automatically handling the specified compression algorithm (None, Brotli, or Gzip).
        /// Ensures efficient decompression by reading the entire stream at once to avoid
        /// performance issues with certain stream implementations.
        /// Throws <see cref="InvalidOperationException"/> if an unknown compression type is encountered.
        /// </summary>
        /// <param name="bytes">The compressed and encoded server message as a byte array.</param>
        /// <returns>The deserialized <see cref="ServerMessage"/> object.</returns>
        internal static ServerMessage DecompressDecodeMessage(byte[] bytes)
        {
            using var stream = new MemoryStream(bytes);

            // The stream will never be empty. It will at least contain the compression algo.
            var compression = (CompressionAlgos)stream.ReadByte();
            // Conditionally decompress and decode.
            Stream decompressedStream = compression switch
            {
                CompressionAlgos.None => stream,
                CompressionAlgos.Brotli => BrotliReader(stream),
                CompressionAlgos.Gzip => GzipReader(stream),
                _ => throw new InvalidOperationException("Unknown compression type"),
            };

            // TODO: consider pooling these.
            // DO NOT TRY TO TAKE THIS OUT. The BrotliStream ReadByte() implementation allocates an array
            // PER BYTE READ. You have to do it all at once to avoid that problem.
            MemoryStream memoryStream = new MemoryStream();
            decompressedStream.CopyTo(memoryStream);
            memoryStream.Seek(0, SeekOrigin.Begin);
            return new ServerMessage.BSATN().Read(new BinaryReader(memoryStream));
        }


        /// <summary>
        /// Decompresses and decodes a <see cref="CompressableQueryUpdate"/> into a <see cref="QueryUpdate"/> object,
        /// automatically handling uncompressed, Brotli, or Gzip-encoded data. Ensures efficient decompression by
        /// reading the entire stream at once to avoid performance issues with certain stream implementations.
        /// Throws <see cref="InvalidOperationException"/> if the compression type is unrecognized.
        /// </summary>
        /// <param name="update">The compressed or uncompressed query update.</param>
        /// <returns>The deserialized <see cref="QueryUpdate"/> object.</returns>
        internal static QueryUpdate DecompressDecodeQueryUpdate(CompressableQueryUpdate update)
        {
            Stream decompressedStream;

            switch (update)
            {
                case CompressableQueryUpdate.Uncompressed(var qu):
                    return qu;

                case CompressableQueryUpdate.Brotli(var bytes):
                    decompressedStream = CompressionHelpers.BrotliReader(new MemoryStream(bytes.ToArray()));
                    break;

                case CompressableQueryUpdate.Gzip(var bytes):
                    decompressedStream = CompressionHelpers.GzipReader(new MemoryStream(bytes.ToArray()));
                    break;

                default:
                    throw new InvalidOperationException();
            }

            // TODO: consider pooling these.
            // DO NOT TRY TO TAKE THIS OUT. The BrotliStream ReadByte() implementation allocates an array
            // PER BYTE READ. You have to do it all at once to avoid that problem.
            MemoryStream memoryStream = new MemoryStream();
            decompressedStream.CopyTo(memoryStream);
            memoryStream.Seek(0, SeekOrigin.Begin);
            return new QueryUpdate.BSATN().Read(new BinaryReader(memoryStream));
        }

        /// <summary>
        /// Prepare to read a BsatnRowList.
        ///
        /// This could return an IEnumerable, but we return the reader and row count directly to avoid an allocation.
        /// It is legitimate to repeatedly call <c>IStructuralReadWrite.Read<T></c> <c>rowCount</c> times on the resulting
        /// BinaryReader:
        /// Our decoding infrastructure guarantees that reading a value consumes the correct number of bytes
        /// from the BinaryReader. (This is easy because BSATN doesn't have padding.)
        ///
        /// Previously here we were using LINQ to do what we're now doing with a custsom reader.
        ///
        /// Why are we no longer using LINQ?
        ///
        /// The calls in question, namely `Skip().Take()`, were fast under the Mono runtime,
        /// but *much* slower when compiled AOT with IL2CPP.
        /// Apparently Mono's JIT is smart enough to optimize away these LINQ ops,
        /// resulting in a linear scan of the `BsatnRowList`.
        /// Unfortunately IL2CPP could not, resulting in a quadratic scan.
        /// See: https://github.com/clockworklabs/com.clockworklabs.spacetimedbsdk/pull/306
        /// </summary>
        /// <param name="list"></param>
        /// <returns>A reader for the rows of the list and a count of rows.</returns>
        internal static (BinaryReader reader, int rowCount) ParseRowList(BsatnRowList list) =>
        (
            new BinaryReader(new ListStream(list.RowsData)),
            list.SizeHint switch
            {
                RowSizeHint.FixedSize(var size) => list.RowsData.Count / size,
                RowSizeHint.RowOffsets(var offsets) => offsets.Count,
                _ => throw new NotImplementedException()
            }
        );
    }
}