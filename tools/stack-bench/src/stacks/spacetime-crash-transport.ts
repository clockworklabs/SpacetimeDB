import type { SpacetimeTarget } from './stack-grading-operations.js';

export interface ReducerReply { outcome: 'committed' | 'refused' | 'unknown' }

// SATS JSON wraps identity values. Connection IDs are u128 JSON numbers;
// preserve their source digits instead of comparing rounded JS numbers.
function correlation(value: unknown, key: '__identity__' | '__connection_id__'): string | null {
  if (!value || typeof value !== 'object' || Object.keys(value).length !== 1 || !(key in value)) return null;
  const text = String((value as Record<string, unknown>)[key]);
  return (key === '__identity__' ? /^0x[0-9a-f]{64}$/.test(text)
    : /^\d+$/.test(text) && BigInt(text) > 0n && BigInt(text) < 2n ** 128n) ? text : null;
}

// Fault diagnostics need the same durable acknowledgement as the browser SDK.
// HTTP /call returns before that confirmation. V1 JSON uses the native websocket
// protocol, explicit confirmed reads, and request IDs; it needs no generated SDK.
export async function openCrashReducerConnection(target: SpacetimeTarget, token: string,
  signal: AbortSignal, connect = (url: string, protocol: string) => new WebSocket(url, protocol)) {
  const url = new URL(`/v1/database/${encodeURIComponent(target.mod)}/subscribe`, target.uri);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  url.searchParams.set('token', token);
  url.searchParams.set('confirmed', 'true');
  url.searchParams.set('compression', 'None');
  let socket: WebSocket;
  try { socket = connect(url.href, 'v1.json.spacetimedb'); }
  catch { throw new Error('confirmed reducer connection could not be created'); }
  let identity = '', connectionId = '', nextId = 0, ready = false;
  const pending = new Map<number, { reducer: string; finish: (result: ReducerReply) => void }>();
  let finishSetup!: () => void, failSetup!: (error: Error) => void;
  const setup = new Promise<void>((resolve, reject) => { finishSetup = resolve; failSetup = reject; });
  const stop = () => {
    signal.removeEventListener('abort', stop);
    if (!ready) failSetup(new Error('confirmed reducer connection did not become ready'));
    for (const { finish } of pending.values()) finish({ outcome: 'unknown' });
    pending.clear();
    socket.close();
  };
  socket.addEventListener('error', stop);
  socket.addEventListener('close', stop, { once: true });
  signal.addEventListener('abort', stop, { once: true });
  const setupTimer = setTimeout(stop, 30_000);
  socket.addEventListener('message', event => {
    try {
      if (typeof event.data !== 'string') throw new Error('expected JSON websocket response');
      const message = JSON.parse(event.data, (key, value, context?: { source?: string }) => {
        if (key !== '__connection_id__' || typeof value !== 'number' || Number.isSafeInteger(value)) return value;
        if (context?.source && /^\d+$/.test(context.source)) return context.source;
        throw new Error('native JSON number cannot be read exactly');
      });
      if (message.IdentityToken) {
        const nextIdentity = correlation(message.IdentityToken.identity, '__identity__');
        const nextConnection = correlation(message.IdentityToken.connection_id, '__connection_id__');
        if (socket.protocol !== 'v1.json.spacetimedb'
          || !nextIdentity || !nextConnection) throw new Error('invalid connection identity');
        identity = nextIdentity;
        connectionId = nextConnection;
        ready = true; finishSetup();
      }
      const update = message.TransactionUpdate;
      const request = pending.get(update?.reducer_call?.request_id);
      if (!request || correlation(update.caller_identity, '__identity__') !== identity
        || correlation(update.caller_connection_id, '__connection_id__') !== connectionId
        || update.reducer_call.reducer_name !== request.reducer) return;
      const status = update.status;
      if (status && typeof status === 'object' && Object.keys(status).length === 1) {
        if (Object.hasOwn(status, 'Committed')) request.finish({ outcome: 'committed' });
        else if (Object.hasOwn(status, 'Failed')) request.finish({ outcome: 'refused' });
        else request.finish({ outcome: 'unknown' });
      } else request.finish({ outcome: 'unknown' });
    } catch { stop(); }
  });
  if (signal.aborted) stop();
  try { await setup; } finally { clearTimeout(setupTimer); }
  return {
    call(reducer: string, args: string, requestSignal: AbortSignal): Promise<ReducerReply> {
      return new Promise(resolve => {
        const requestId = ++nextId;
        const finish = (reply: ReducerReply) => {
          requestSignal.removeEventListener('abort', abort);
          pending.delete(requestId);
          resolve(reply);
        };
        const abort = () => finish({ outcome: 'unknown' });
        if (requestSignal.aborted || socket.readyState !== WebSocket.OPEN) return finish({ outcome: 'unknown' });
        pending.set(requestId, { reducer, finish });
        requestSignal.addEventListener('abort', abort, { once: true });
        try { socket.send(JSON.stringify({ CallReducer: { reducer, args, request_id: requestId, flags: 0 } })); }
        catch { finish({ outcome: 'unknown' }); }
      });
    },
    close: stop,
  };
}
