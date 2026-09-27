import { brotliDecompressSync, gunzipSync } from 'node:zlib';
import { createHash } from 'node:crypto';
import type { Page, Request, Response } from 'playwright';
import { inconclusive } from '../src/actions/actor-action-runtime.js';

const MAX_RECEIVED_BYTES = 8 * 1024 * 1024;

export function requestDiagnostic(request: Request) {
  const url = new URL(request.url());
  return { origin: url.origin, pathSha256: createHash('sha256').update(url.pathname).digest('hex'),
    method: request.method(), resourceType: request.resourceType() };
}

function responseDiagnostic(response: Response, page: Page) {
  return { ...requestDiagnostic(response.request()),
    status: response.status(), contentType: response.headers()['content-type'] ?? '',
    pageClosed: page.isClosed(),
    failure: response.request().failure()?.errorText ?? null };
}

function bodyReadErrorCategory(error: unknown): string {
  // Browser errors can include private URLs or payloads. Emit only fixed labels.
  const message = error instanceof Error ? error.message : '';
  if (/evicted from inspector cache/i.test(message)) return 'body-evicted';
  if (/No resource with given identifier|No data found for (?:resource with )?given identifier/i.test(message)) return 'resource-unavailable';
  if (/Target page, context or browser has been closed|Session closed/i.test(message)) return 'target-closed';
  if (/net::ERR_BLOCKED_BY_ORB\b/.test(message)) return 'blocked-by-orb';
  if (/net::ERR_[A-Z_]+\b/.test(message)) return 'request-failed';
  if (/Protocol error \(/.test(message)) return 'protocol-error';
  return 'unknown';
}

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
  private readonly incompleteCounts = { byteLimit: 0, bodyReadFailures: 0, unsupportedStreams: 0, navigationInterrupted: 0 };
  incomplete = false;
  pending = 0;
  readonly pendingResponses = new Map<Response, { page: Page; startedAt: number }>();

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
    if (requireComplete && (this.incomplete || this.pending)) {
      for (const [response, { page, startedAt }] of [...this.pendingResponses].slice(0, 8)) {
        try { process.stderr.write(`transport body pending ${JSON.stringify({
          ...responseDiagnostic(response, page), elapsedMs: Date.now() - startedAt,
        })}\n`); } catch { /* Diagnostics must not change the verdict. */ }
      }
      inconclusive('transport-incomplete', {
        capture: { ...this.incompleteCounts, pendingBodies: this.pending, retainedBytes: this.bytes },
      });
    }
    return false;
  }
}

export async function captureResponses(page: Page, received: ReceivedTransport, freshResponses = false): Promise<void> {
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
    received.pendingResponses.set(response, { page, startedAt: Date.now() });
    try { received.record(await response.text()); }
    catch (error) {
      received.markIncomplete('bodyReadFailures');
      if (reportedBodyFailures++ < 8) {
        try {
          process.stderr.write(`transport body unavailable ${JSON.stringify({
            ...responseDiagnostic(response, page),
            error: bodyReadErrorCategory(error),
          })}\n`);
        } catch { /* Diagnostics must not turn incomplete capture into a process failure. */ }
      }
    }
    finally { received.pending--; received.pendingResponses.delete(response); }
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
  // Chromium discards decoded font bytes; a later memory-cache hit has no body
  // to inspect. Privacy absence observations require fresh HTTP responses.
  if (freshResponses) await session.send('Network.setCacheDisabled', { cacheDisabled: true });
  // Keep response bodies available when the app reloads immediately after reading them.
  await session.send('Network.configureDurableMessages', {
    maxTotalBufferSize: MAX_RECEIVED_BYTES, maxResourceBufferSize: MAX_RECEIVED_BYTES,
  });
}
