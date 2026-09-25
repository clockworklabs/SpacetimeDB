import { brotliDecompressSync, gunzipSync } from 'node:zlib';
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import type { Page, WebSocketRoute } from 'playwright';
import { leasedSpacetimeTarget } from '../../runtime/spacetime-target.js';
import type { LeasedSpacetimeTarget } from '../../runtime/spacetime-target.js';
import { inconclusive } from '../../actions/actor-action-runtime.js';

interface Reader { readonly offset: number; readonly remaining: number }
interface Writer { getBuffer(): Uint8Array }
interface Call { requestId: number; flags: number; reducer: string; args: Uint8Array }
interface Message {
  tag: string;
  value: Call & { result: { tag: string } };
}
interface MessageCodec {
  deserialize(reader: Reader): Message;
  serialize(writer: Writer, message: Message): void;
}
interface Codec {
  BinaryReader: new (bytes: Uint8Array) => Reader;
  BinaryWriter: new (size: number) => Writer;
  ClientMessage: MessageCodec;
  ServerMessage: MessageCodec;
  AlgebraicType: {
    makeDeserializer(type: { tag: string }): (reader: Reader) => unknown;
    makeSerializer(type: { tag: string }): (writer: Writer, value: unknown) => void;
  };
}
interface CapturedCall { call: Call; committed: boolean }
interface Socket {
  url: string;
  server: WebSocketRoute;
  closed: boolean;
  invalid: boolean;
  identified: boolean;
  calls: CapturedCall[];
  ids: Set<number>;
  nextId: number;
  pending?: { id: number; finish(result: string): void };
  authPending?: { id: number; finish(result: string): void };
}
interface Template { socket: Socket; call: Call; args: unknown[]; at: number; encode(args: unknown[]): Uint8Array }
interface Capture {
  page: Page; codec: Codec; sockets: Socket[];
  handshakes: Map<string, { url: string; protocol?: string }>;
  template?: { match: string; control: string; count: number; value: Template };
  auth?: { change(message: Message, socket: Socket): Message | null; fail(): void };
}
type AuthReceipt = { shape: string; success: boolean; transport: 'spacetime-websocket';
  bodySha256: string; absentParameters?: string[] };
const captures = new WeakMap<object, Capture>();
let codec: Promise<Codec> | undefined;

function encode(codec: Codec, type: MessageCodec, message: Message): Buffer {
  const writer = new codec.BinaryWriter(512); type.serialize(writer, message); return Buffer.from(writer.getBuffer());
}
function decode(codec: Codec, type: MessageCodec, bytes: string | Buffer): Message[] {
  if (!Buffer.isBuffer(bytes) || !bytes.length || bytes.length > 32 * 1024 * 1024) throw new Error('Unsupported frame');
  const reader = new codec.BinaryReader(bytes), messages: Message[] = [];
  while (reader.remaining) {
    const start = reader.offset, message = type.deserialize(reader);
    if (reader.offset <= start || !encode(codec, type, message).equals(bytes.subarray(start, reader.offset))) throw new Error('Frame round trip failed');
    messages.push(message);
  }
  return messages;
}
function serverBytes(bytes: string | Buffer): Buffer {
  if (!Buffer.isBuffer(bytes)) throw new Error('Unsupported frame');
  const options = { maxOutputLength: 32 * 1024 * 1024 };
  if (bytes[0] === 0) return bytes.subarray(1);
  if (bytes[0] === 1) return brotliDecompressSync(bytes.subarray(1), options);
  if (bytes[0] === 2) return gunzipSync(bytes.subarray(1), options);
  throw new Error('Unsupported compression');
}

function leasedSocket(capture: Capture, socket: Socket, target: LeasedSpacetimeTarget,
  allowAppProxy = false): boolean {
  const url = new URL(socket.url);
  url.protocol = url.protocol === 'wss:' ? 'https:' : 'http:';
  const handshakes = [...capture.handshakes.values()].filter(item => item.url === socket.url);
  const path = `/v1/database/${target.mod}/subscribe`;
  const targetUrl = new URL(target.uri);
  const direct = url.pathname === path && url.protocol === targetUrl.protocol && url.port === targetUrl.port
    && (url.hostname === targetUrl.hostname
      || url.hostname === 'localhost' && targetUrl.hostname === '127.0.0.1');
  const appProxy = allowAppProxy && url.origin === new URL(capture.page.url()).origin
    && (url.pathname === path || url.pathname === `/db${path}`);
  return !socket.closed && !socket.invalid && socket.identified
    && (direct || appProxy)
    && handshakes.length === 1
    && ['v2.bsatn.spacetimedb', 'v3.bsatn.spacetimedb'].includes(handshakes[0]!.protocol ?? '');
}

// Only selected repeatFormWrite actors use this route. Forward all application
// traffic unchanged, including replies for native setup writes, on the SAME socket.
export async function installSpacetimeWriteCapture(page: Page): Promise<void> {
  if (captures.has(page)) return;
  const wire = await (codec ??= import(new URL('../spacetime-wire-codec.js', import.meta.url).href));
  const capture: Capture = { page, codec: wire, sockets: [], handshakes: new Map() }; captures.set(page, capture);
  // Read the actual server handshake outside the app's mutable JavaScript realm.
  const inspector = await page.context().newCDPSession(page);
  inspector.on('Network.webSocketCreated', event => {
    capture.handshakes.set(event.requestId, { url: event.url });
  });
  inspector.on('Network.webSocketHandshakeResponseReceived', event => {
    const handshake = capture.handshakes.get(event.requestId);
    if (handshake && event.response.status === 101) handshake.protocol = Object.entries(event.response.headers)
      .find(([key]) => key.toLowerCase() === 'sec-websocket-protocol')?.[1];
  });
  inspector.on('Network.webSocketClosed', event => { capture.handshakes.delete(event.requestId); });
  page.once('close', () => { void inspector.detach().catch(() => {}); });
  await inspector.send('Network.enable');
  await page.routeWebSocket(/\/v1\/database\/[^/]+\/subscribe(?:\?|$)/, client => {
    const server = client.connectToServer();
    const socket: Socket = { url: client.url(), server, closed: false, invalid: false,
      identified: false, calls: [], ids: new Set(), nextId: 0xffffffff };
    capture.sockets = capture.sockets.filter(item => !item.closed);
    capture.sockets.push(socket);
    const invalidate = () => {
      socket.invalid = true;
      socket.pending?.finish('unknown');
      socket.authPending?.finish('unknown');
      capture.auth?.fail();
    };
    client.onMessage(bytes => {
      let outbound = bytes;
      try {
        let changedFrame = false;
        const messages = decode(wire, wire.ClientMessage, bytes);
        for (const message of messages) {
          const id = message.value?.requestId;
          if (id !== undefined) {
            if (socket.ids.has(id) || socket.ids.size >= 10000) invalidate();
            socket.ids.add(id);
          }
          if (message.tag === 'CallReducer') {
            const changed = capture.auth?.change(message, socket);
            if (changed) {
              message.value = changed.value;
              changedFrame = true;
            }
            if (socket.calls.length >= 200) invalidate();
            else socket.calls.push({ call: message.value, committed: false });
          }
        }
        if (changedFrame) outbound = Buffer.concat(messages.map(message => encode(wire, wire.ClientMessage, message)));
      } catch {
        const patchActive = Boolean(capture.auth);
        invalidate();
        if (patchActive) return;
      }
      try { server.send(outbound); } catch { invalidate(); }
    });
    server.onMessage(bytes => {
      try {
        for (const message of decode(wire, wire.ServerMessage, serverBytes(bytes))) {
          if (message.tag === 'InitialConnection') {
            if (socket.identified) invalidate();
            socket.identified = true;
          }
          if (message.tag === 'ReducerResult') {
            const { requestId, result } = message.value;
            for (const item of socket.calls) if (item.call.requestId === requestId)
              item.committed = result.tag === 'Ok' || result.tag === 'OkEmpty';
            if (socket.pending?.id === requestId) socket.pending.finish(result.tag);
            if (socket.authPending?.id === requestId) socket.authPending.finish(result.tag);
          }
        }
      } catch { invalidate(); }
      try { client.send(bytes); } catch { invalidate(); }
    });
    client.onClose(async (code, reason) => { socket.closed = true; invalidate(); await server.close({ code, reason }).catch(invalidate); });
    server.onClose(async (code, reason) => { socket.closed = true; invalidate(); await client.close({ code, reason }).catch(invalidate); });
  });
}

// Patch the app's own credential reducer call on its existing socket. The
// declared reducer parameters are the only authority for locating extra fields.
export async function startSpacetimeAuthPatch(page: object, username: string, password: string,
  patch: (args: unknown[], parameters: readonly { name: string }[]) =>
    { body: string; shape: string; absentParameters?: string[] } | null) {
  const capture = captures.get(page);
  if (!capture) return null;
  if (capture.auth) throw new Error('Authentication request patch is already active');
  const target = leasedSpacetimeTarget();
  const schemaUrl = new URL(`/v1/database/${target.mod}/schema?version=9`, target.uri);
  let schema: { reducers?: { name: string; params?: { elements?: { name?: { some?: string };
    algebraic_type?: Record<string, unknown> }[] } }[]; typespace?: { types?: Record<string, unknown>[] } };
  try {
    const response = await capture.page.request.get(schemaUrl.href, { timeout: 10_000 });
    if (!response.ok()) return { receipt: async () => undefined, dispose: () => {} };
    schema = await response.json();
    if (!schema || typeof schema !== 'object') return { receipt: async () => undefined, dispose: () => {} };
  } catch { return { receipt: async () => undefined, dispose: () => {} }; }
  const stringType = (raw: Record<string, unknown> | undefined, depth = 0): boolean => {
    if (!raw || depth > 8) return false;
    return typeof raw.Ref === 'number'
      ? stringType(schema.typespace?.types?.[raw.Ref], depth + 1)
      : Object.keys(raw).length === 1 && Object.hasOwn(raw, 'String');
  };
  const declarations = new Map<string, { names: string[]; read: ((reader: Reader) => unknown)[];
    write: ((writer: Writer, value: unknown) => void)[] }>();
  for (const reducer of schema.reducers ?? []) {
    if (!/^(signup|signin)$/i.test(reducer.name.replaceAll('_', ''))) continue;
    const fields = reducer.params?.elements;
    if (!fields?.length || !fields.every(field => stringType(field.algebraic_type)
      && typeof field.name?.some === 'string')) continue;
    const type = { tag: 'String' };
    declarations.set(reducer.name, {
      names: fields.map(field => field.name!.some!),
      read: fields.map(() => capture.codec.AlgebraicType.makeDeserializer(type)),
      write: fields.map(() => capture.codec.AlgebraicType.makeSerializer(type)),
    });
  }
  let matched = false, settled = false, timer: ReturnType<typeof setTimeout> | undefined;
  let finish!: (value: AuthReceipt | undefined) => void;
  const done = new Promise<AuthReceipt | undefined>(resolve => {
    finish = value => { if (settled) return; settled = true; clearTimeout(timer); resolve(value); };
  });
  capture.auth = {
    change(message, socket) {
      const declaration = declarations.get(message.value.reducer);
      if (!declaration) return null;
      const reader = new capture.codec.BinaryReader(message.value.args);
      const values = declaration.read.map(read => read(reader));
      if (reader.remaining || values.filter(value => value === username).length !== 1
        || values.filter(value => value === password).length !== 1) return null;
      if (!leasedSocket(capture, socket, target, true)) return null;
      const writer = new capture.codec.BinaryWriter(128);
      declaration.write.forEach((write, index) => write(writer, values[index]));
      if (!Buffer.from(writer.getBuffer()).equals(Buffer.from(message.value.args))
        || matched || socket.authPending || socket.invalid) { finish(undefined); throw new Error('Ambiguous credential call'); }
      const changed = patch(values, declaration.names.map(name => ({ name })));
      if (!changed) return null;
      const args = JSON.parse(changed.body) as unknown[];
      if (!Array.isArray(args) || args.length !== values.length || args.some(value => typeof value !== 'string')) {
        finish(undefined); throw new Error('Unsupported credential arguments');
      }
      const encoded = new capture.codec.BinaryWriter(128);
      declaration.write.forEach((write, index) => write(encoded, args[index]));
      const sent = { ...message, value: { ...message.value, args: encoded.getBuffer() } };
      const bodySha256 = createHash('sha256').update(encode(capture.codec, capture.codec.ClientMessage, sent)).digest('hex');
      matched = true;
      socket.authPending = { id: message.value.requestId, finish(result) {
        socket.authPending = undefined;
        finish(['Ok', 'OkEmpty', 'Err'].includes(result) && !socket.invalid
          ? { shape: changed.shape, success: result !== 'Err', transport: 'spacetime-websocket',
              bodySha256, ...(changed.absentParameters ? { absentParameters: changed.absentParameters } : {}) }
          : undefined);
      } };
      timer = setTimeout(() => socket.authPending?.finish('unknown'), 30_000);
      return sent;
    },
    fail: () => finish(undefined),
  };
  return {
    receipt: async () => matched ? done : undefined,
    dispose: () => { capture.auth = undefined; finish(undefined); },
  };
}

async function template(capture: Capture, match: string, control: string, signal: AbortSignal): Promise<Template | null> {
  const live = capture.sockets.filter(socket => !socket.closed);
  if (live.length !== 1 || !match || match === control) return null;
  const socket = live[0]!;
  if (socket.invalid || !socket.identified || socket.pending) return null;
  const target = leasedSpacetimeTarget(), url = new URL(socket.url);
  url.protocol = url.protocol === 'wss:' ? 'https:' : 'http:';
  if (!leasedSocket(capture, socket, target)) return null;
  const cached = capture.template;
  if (cached?.value.socket === socket && cached.match === match && cached.control === control && cached.count === socket.calls.length) return cached.value;
  signal.throwIfAborted();
  url.pathname = `/v1/database/${target.mod}/schema`; url.search = '?version=9';
  const response = await capture.page.request.get(url.href, { timeout: 10000 });
  if (!response.ok()) return null;
  const schema = await response.json();
  // ponytail: scalar reducer parameters only; unsupported shapes retain UI setup.
  const scalars = new Set(['Bool', 'I8', 'U8', 'I16', 'U16', 'I32', 'U32', 'I64', 'U64', 'I128', 'U128', 'I256', 'U256', 'F32', 'F64', 'String']);
  const scalar = (raw: Record<string, unknown>, depth = 0): { tag: string } => {
    if (!raw || depth > 8) throw new Error('Unsupported parameter');
    if ('Ref' in raw) return scalar(schema.typespace?.types?.[Number(raw.Ref)], depth + 1);
    const tags = Object.keys(raw);
    if (tags.length !== 1 || !scalars.has(tags[0]!)) throw new Error('Unsupported parameter');
    return { tag: tags[0]! };
  };
  const declarations: { name: string; params: { elements: { algebraic_type: Record<string, unknown> }[] } }[] =
    Array.isArray(schema?.reducers) ? schema.reducers : [];
  const found: Template[] = [];
  for (const declaration of declarations) {
    try {
      const types = declaration.params.elements.map(field => scalar(field.algebraic_type));
      const readers = types.map(type => capture.codec.AlgebraicType.makeDeserializer(type));
      const writers = types.map(type => capture.codec.AlgebraicType.makeSerializer(type));
      const encodeArgs = (args: unknown[]) => {
        const writer = new capture.codec.BinaryWriter(128);
        writers.forEach((write, index) => write(writer, args[index])); return writer.getBuffer();
      };
      const calls = socket.calls.filter(item => item.call.reducer === declaration.name).map(item => {
        const reader = new capture.codec.BinaryReader(item.call.args), args = readers.map(read => read(reader));
        if (reader.remaining || !Buffer.from(encodeArgs(args)).equals(Buffer.from(item.call.args))) throw new Error('Argument round trip failed');
        return { ...item, args };
      });
      const a = calls.filter(item => item.args.includes(match)), b = calls.filter(item => item.args.includes(control));
      if (a.length !== 1 || b.length !== 1 || !a[0]!.committed || !b[0]!.committed) continue;
      const first = a[0]!, second = b[0]!, at = first.args.indexOf(match);
      if (first.args.filter(value => value === match).length !== 1 || second.args.filter(value => value === control).length !== 1
        || second.args[at] !== control || first.call.flags !== 0 || second.call.flags !== 0
        || !isDeepStrictEqual(first.args.map((value, index) => index === at ? control : value), second.args)) continue;
      found.push({ socket, call: second.call, args: second.args, at, encode: encodeArgs });
    } catch { /* Unsupported parameter shapes or captures keep the form path. */ }
  }
  if (found.length !== 1) return null;
  capture.template = { match, control, count: socket.calls.length, value: found[0]! };
  return found[0]!;
}

// null means nothing was sent. After send, uncertainty must never become UI retry.
export async function repeatSpacetimeWrite(page: object, match: string, control: string, replacement: string, signal: AbortSignal) {
  const capture = captures.get(page);
  if (!capture) return null;
  let selected: Template | null;
  try { selected = await template(capture, match, control, signal); }
  catch { signal.throwIfAborted(); return null; }
  if (!selected) return null;
  const { socket } = selected;
  if (socket.closed || socket.invalid || socket.pending || capture.sockets.filter(item => !item.closed).length !== 1) return null;
  signal.throwIfAborted();
  while (socket.nextId >= 0 && socket.ids.has(socket.nextId)) socket.nextId--;
  if (socket.nextId < 0 || socket.ids.size >= 10000) return null;
  const id = socket.nextId--, args = [...selected.args]; args[selected.at] = replacement;
  const bytes = encode(capture.codec, capture.codec.ClientMessage, { tag: 'CallReducer',
    value: { ...selected.call, requestId: id, args: selected.encode(args) } } as Message);
  socket.ids.add(id);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const abort = () => socket.pending?.finish('unknown');
  try {
    const receipt = new Promise<string>(resolve => {
      socket.pending = { id, finish: resolve };
      timer = setTimeout(abort, 10000);
      signal.addEventListener('abort', abort, { once: true });
    });
    socket.server.send(bytes);
    const result = await receipt;
    if (socket.invalid || socket.closed || signal.aborted || !['Ok', 'OkEmpty', 'Err'].includes(result)) inconclusive('transport-incomplete', {});
    return { method: 'spacetime-reducer', accepted: result !== 'Err', result };
  } catch { inconclusive('transport-incomplete', {}); }
  finally { clearTimeout(timer); signal.removeEventListener('abort', abort); socket.pending = undefined; }
}
