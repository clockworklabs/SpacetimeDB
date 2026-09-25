import { brotliDecompressSync, gunzipSync } from 'node:zlib';
import { createHash } from 'node:crypto';
import type { Page } from 'playwright';
import { inconclusive } from '../src/actions/actor-action-runtime.js';

const MAX_RECEIVED_BYTES = 8 * 1024 * 1024;

// A SpacetimeDB server frame carries a one-byte compression tag ahead of the
// message: 0 none, 1 brotli, 2 gzip, and the SDK compresses by default. The
// message text is inline UTF-8 once decoded, so a substring search finds it
// without the harness knowing the wire format. Any other frame is kept as it
// arrived.
export function transportFrameText(payload: string | Buffer): string {
  if (typeof payload === 'string') return payload;
  const bytes = Buffer.from(payload);
  if (bytes.length > 1) {
    try {
      if (bytes[0] === 1) return brotliDecompressSync(bytes.subarray(1)).toString('utf8');
      if (bytes[0] === 2) return gunzipSync(bytes.subarray(1)).toString('utf8');
    } catch { /* not a compressed SpacetimeDB frame */ }
  }
  return bytes.toString('utf8');
}

// Bounded evidence must never turn dropped data into a privacy pass.
export class ReceivedTransport {
  readonly chunks: string[] = [];
  private bytes = 0;
  private readonly incompleteCounts = { byteLimit: 0, bodyReadFailures: 0, unsupportedStreams: 0 };
  incomplete = false;
  pending = 0;

  constructor(private readonly limit = MAX_RECEIVED_BYTES) {}

  markIncomplete(reason: keyof ReceivedTransport['incompleteCounts']): void {
    this.incomplete = true;
    this.incompleteCounts[reason]++;
  }

  record(payload: string | Buffer): void {
    const text = transportFrameText(payload);
    // Receipt checks need presence, not frequency. Reloading an identical bundle
    // adds no evidence and must not evict distinct data.
    if (!text || this.chunks.includes(text)) return;
    if (Buffer.byteLength(text) > this.limit) {
      this.markIncomplete('byteLimit');
      return;
    }
    this.chunks.push(text);
    this.bytes += Buffer.byteLength(text);
    while (this.bytes > this.limit) {
      this.markIncomplete('byteLimit');
      this.bytes -= Buffer.byteLength(this.chunks.shift()!);
    }
  }

  contains(needle: string, requireComplete = true): boolean {
    if (this.chunks.some(chunk => chunk.includes(needle))) return true;
    if (requireComplete && (this.incomplete || this.pending)) inconclusive('transport-incomplete', {
      capture: { ...this.incompleteCounts, pendingBodies: this.pending, retainedBytes: this.bytes },
    });
    return false;
  }
}

export async function captureResponses(page: Page, received: ReceivedTransport): Promise<void> {
  let reportedBodyFailures = 0;
  page.on('response', async response => {
    const type = response.headers()['content-type'] ?? '';
    // Native EventSource messages are captured below without waiting for stream closure.
    if (/text\/event-stream/.test(type)) return;
    // Public scripts and styles can contain secrets too. Keep the same bounded,
    // fail-closed capture for data, rendered pages, assets and error responses.
    if (!/(application\/(json|[^;]+\+json|x-ndjson|(?:x-)?(?:java|ecma)script)|text\/(plain|html|css|(?:java|ecma)script))/i.test(type)) return;
    if (Number(response.headers()['content-length']) > MAX_RECEIVED_BYTES) {
      received.markIncomplete('byteLimit');
      return;
    }
    received.pending++;
    try { received.record(await response.text()); }
    catch (error) {
      received.markIncomplete('bodyReadFailures');
      if (reportedBodyFailures++ < 8) {
        const url = new URL(response.url());
        process.stderr.write(`transport body unavailable ${JSON.stringify({
          origin: url.origin, pathSha256: createHash('sha256').update(url.pathname).digest('hex'),
          status: response.status(), contentType: type, resourceType: response.request().resourceType(),
          pageClosed: page.isClosed(), failure: response.request().failure()?.errorText ?? null,
          error: error instanceof Error ? error.name : typeof error,
        })}\n`);
      }
    }
    finally { received.pending--; }
  });
  const session = await page.context().newCDPSession(page);
  session.on('Network.eventSourceMessageReceived', event => received.record(event.data));
  session.on('Network.responseReceived', event => {
    // Fetch streams have no EventSource events. Absence of an unseen body is not a pass.
    if (event.response.mimeType === 'text/event-stream' && event.type !== 'EventSource') {
      received.markIncomplete('unsupportedStreams');
    }
  });
  await session.send('Network.enable');
  // Keep response bodies available when the app reloads immediately after reading them.
  await session.send('Network.configureDurableMessages', {
    maxTotalBufferSize: MAX_RECEIVED_BYTES, maxResourceBufferSize: MAX_RECEIVED_BYTES,
  });
}
