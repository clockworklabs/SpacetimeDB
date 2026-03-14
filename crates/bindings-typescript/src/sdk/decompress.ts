export async function decompress(
  buffer: Uint8Array<ArrayBuffer>,
  type: CompressionFormat,
  chunkSize: number = 128 * 1024 // 128KB
): Promise<Uint8Array> {
  // Create a single ReadableStream to handle chunks
  let offset = 0;
  const readableStream = new ReadableStream<BufferSource>({
    pull(controller) {
      if (offset < buffer.length) {
        // Slice a chunk of the buffer and enqueue it
        const chunk = buffer.subarray(
          offset,
          Math.min(offset + chunkSize, buffer.length)
        );
        controller.enqueue(chunk);
        offset += chunkSize;
      } else {
        controller.close();
      }
    },
  });

  // Create a single DecompressionStream
  const decompressionStream = new DecompressionStream(type);

  // Pipe the ReadableStream through the DecompressionStream
  const decompressedStream = readableStream.pipeThrough(decompressionStream);

  // Collect the decompressed chunks efficiently
  const chunks = [];
  for await (const chunk of decompressedStream) {
    chunks.push(chunk);
  }
  return new Blob(chunks).bytes();
}
