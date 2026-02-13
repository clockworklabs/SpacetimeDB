export async function decompress(
  buffer: Uint8Array,
  // Leaving it here to expand to brotli when it lands in the browsers and NodeJS
  type: 'gzip',
  chunkSize: number = 128 * 1024 // 128KB
): Promise<Uint8Array> {
  // Create a single ReadableStream to handle chunks
  let offset = 0;
  const readableStream = new ReadableStream({
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
  const reader = decompressedStream.getReader();
  const chunks: Uint8Array[] = [];
  let totalLength = 0;
  let result: any;

  while (!(result = await reader.read()).done) {
    chunks.push(result.value);
    totalLength += result.value.length;
  }

  // Allocate a single Uint8Array for the decompressed data
  const decompressedArray = new Uint8Array(totalLength);
  let chunkOffset = 0;

  for (const chunk of chunks) {
    decompressedArray.set(chunk, chunkOffset);
    chunkOffset += chunk.length;
  }

  return decompressedArray;
}
