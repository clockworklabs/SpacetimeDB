import { brotliDecompressSync, gunzipSync } from 'node:zlib';
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import type { Page, WebSocketRoute } from 'playwright';
import { leasedSpacetimeTarget } from '../../runtime/spacetime-target.js';
import type { LeasedSpacetimeTarget } from '../../runtime/spacetime-target.js';
import { inconclusive } from '../../actions/actor-action-runtime.js';
import { withBrowserRequest } from '../../actions/browser-request.js';

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
interface CapturedCall { call: Call; committed: boolean; result?: string; observation?: AuthObservation }
type AuthKind = 'signup' | 'signin';
interface AuthObservation {
  calls: CapturedCall[]; failed: boolean; finishWait(): void;
}
interface AuthWitness { kind: AuthKind; user: string; item: CapturedCall; accepted: boolean; usable: boolean }
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
  authResult?: (socket: Socket, id: number, result: string) => void;
  authProcedure?: () => void;
  authObservation?: AuthObservation;
  authWitnesses: AuthWitness[];
  authUnproved: Set<AuthKind>;
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
  allowAppProxy = false, allowClosedHandshakeOverlap = false): boolean {
  const url = new URL(socket.url);
  url.protocol = url.protocol === 'wss:' ? 'https:' : 'http:';
  const handshakes = [...capture.handshakes.values()].filter(item => item.url === socket.url);
  const path = `/v1/database/${target.mod}/subscribe`;
  const targetUrl = new URL(target.uri);
  const direct = url.pathname === path && url.protocol === targetUrl.protocol && url.port === targetUrl.port
    && (url.hostname === targetUrl.hostname
      || url.hostname === 'localhost' && targetUrl.hostname === '127.0.0.1');
  // The app chooses its proxy prefix. Keep the exact leased database suffix.
  const appProxy = allowAppProxy && url.origin === new URL(capture.page.url()).origin
    && url.pathname.endsWith(path);
  const handshakesProved = handshakes.length === 1
    || allowClosedHandshakeOverlap && handshakes.length > 1
      && capture.sockets.filter(candidate => !candidate.closed && candidate.url === socket.url).length === 1;
  return !socket.closed && !socket.invalid && socket.identified
    && (direct || appProxy)
    && handshakesProved
    && handshakes.every(item => ['v2.bsatn.spacetimedb', 'v3.bsatn.spacetimedb'].includes(item.protocol ?? ''));
}

// Only selected repeatFormWrite actors use this route. Forward all application
// traffic unchanged, including replies for native setup writes, on the SAME socket.
export async function installSpacetimeWriteCapture(page: Page): Promise<void> {
  if (captures.has(page)) return;
  const wire = await (codec ??= import(new URL('../spacetime-wire-codec.js', import.meta.url).href));
  const capture: Capture = { page, codec: wire, sockets: [], handshakes: new Map(),
    authWitnesses: [], authUnproved: new Set() }; captures.set(page, capture);
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
  page.on('request', request => {
    const observation = capture.authObservation;
    if (observation && ['POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method())) {
      // SDK reconnect only re-signs an existing identity for its WebSocket.
      let transportToken = false;
      try {
        const target = new URL(leasedSpacetimeTarget().uri), url = new URL(request.url());
        transportToken = request.method() === 'POST' && url.pathname === '/v1/identity/websocket-token'
          && !url.search && !request.postData() && url.protocol === target.protocol && url.port === target.port
          && (url.hostname === target.hostname || url.hostname === 'localhost' && target.hostname === '127.0.0.1');
      } catch { /* An unproved request remains a competing write. */ }
      if (transportToken) return;
      observation.failed = true;
      observation.finishWait();
    }
  });
  page.once('close', () => {
    capture.authObservation = undefined;
    for (const socket of capture.sockets) for (const item of socket.calls) {
      if (item.observation && !item.result) { item.observation.failed = true; item.observation.finishWait(); }
    }
    capture.authWitnesses = [];
    capture.authUnproved.clear();
    void inspector.detach().catch(() => {});
  });
  await inspector.send('Network.enable');
  await page.routeWebSocket(/\/v1\/database\/[^/]+\/subscribe(?:\?|$)/, client => {
    const server = client.connectToServer();
    const socket: Socket = { url: client.url(), server, closed: false, invalid: false,
      identified: false, calls: [], ids: new Set(), nextId: 0xffffffff };
    capture.sockets = capture.sockets.filter(item => !item.closed);
    capture.sockets.push(socket);
    const invalidate = () => {
      socket.invalid = true;
      for (const item of socket.calls) if (item.observation && !item.result) {
        item.observation.failed = true;
        item.observation.finishWait();
      }
      socket.pending?.finish('unknown');
      socket.authPending?.finish('unknown');
      capture.auth?.fail();
    };
    const close = () => {
      socket.closed = true;
      socket.invalid = true;
      for (const item of socket.calls) if (item.observation && !item.result) {
        item.observation.failed = true;
        item.observation.finishWait();
      }
      socket.pending?.finish('unknown');
      socket.authPending?.finish('unknown');
    };
    client.onMessage(bytes => {
      let outbound = bytes;
      try {
        let changedFrame = false;
        const messages = decode(wire, wire.ClientMessage, bytes);
        for (const message of messages) {
          if (message.tag === 'CallProcedure') capture.authProcedure?.();
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
            else {
              const item: CapturedCall = { call: message.value, committed: false };
              const observation = capture.authObservation;
              if (observation) {
                if (!leasedSocket(capture, socket, leasedSpacetimeTarget(), true, true)) observation.failed = true;
                item.observation = observation;
                observation.calls.push(item);
              }
              socket.calls.push(item);
            }
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
            capture.authResult?.(socket, requestId, result.tag);
            for (const item of socket.calls) if (item.call.requestId === requestId) {
              item.committed = result.tag === 'Ok' || result.tag === 'OkEmpty';
              item.result = result.tag;
              item.observation?.finishWait();
              item.observation = undefined;
            }
            if (socket.pending?.id === requestId) socket.pending.finish(result.tag);
            if (socket.authPending?.id === requestId) socket.authPending.finish(result.tag);
          }
        }
      } catch { invalidate(); }
      try { client.send(bytes); } catch { invalidate(); }
    });
    client.onClose(async (code, reason) => { close(); await server.close({ code, reason }).catch(close); });
    server.onClose(async (code, reason) => { close(); await client.close({ code, reason }).catch(close); });
  });
}

// Record the one native call made by an ordinary form submit. The caller must
// verify the account state before this can identify a later modified request.
export function beginSpacetimeAuthObservation(page: object, kind: AuthKind, user: string) {
  const capture = captures.get(page);
  if (!capture) return null;
  if (capture.auth) return null;
  if (capture.authObservation) throw new Error('Authentication observation is already active');
  let finishWait!: () => void;
  const done = new Promise<void>(resolve => { finishWait = resolve; });
  const observation: AuthObservation = { calls: [], failed: false, finishWait };
  capture.authObservation = observation;
  let completed = false;
  const discard = () => {
    if (completed) return false;
    completed = true;
    if (capture.authObservation === observation) capture.authObservation = undefined;
    if (observation.calls.length) capture.authUnproved.add(kind);
    for (const socket of capture.sockets) {
      socket.calls = socket.calls.filter(call => !observation.calls.includes(call));
    }
    observation.calls.length = 0;
    return false;
  };
  return {
    stop: () => { if (capture.authObservation === observation) capture.authObservation = undefined; },
    discard,
    finish: async (accepted: boolean, uiVerified = true) => {
      if (completed) return false;
      if (capture.authObservation === observation) capture.authObservation = undefined;
      const item = observation.calls.length === 1 ? observation.calls[0] : undefined;
      if (!uiVerified || !item || observation.failed) return discard();
      const timeout = setTimeout(finishWait, 10_000);
      try { if (!item.result) await done; } finally { clearTimeout(timeout); }
      const valid = !observation.failed && ['Ok', 'OkEmpty', 'Err'].includes(item.result ?? '')
        && (!accepted || item.committed);
      if (!valid) return discard();
      if (capture.authWitnesses.length >= 3) {
        for (const socket of capture.sockets) {
          socket.calls = socket.calls.filter(call => !capture.authWitnesses.some(witness => witness.item === call));
        }
        capture.authWitnesses = [];
        return discard();
      }
      capture.authWitnesses.push({ kind, user, item, accepted, usable: kind === 'signin' && accepted });
      completed = true;
      observation.calls.length = 0;
      return true;
    },
  };
}

export function confirmSpacetimeSignup(page: object, user: string): void {
  const capture = captures.get(page);
  if (!capture || capture.authUnproved.has('signup')) return;
  for (const witness of capture.authWitnesses) {
    if (witness.kind === 'signup' && witness.user === user && witness.item.committed) witness.usable = true;
  }
}

async function authDeclarations(capture: Capture) {
  const target = leasedSpacetimeTarget();
  const schemaUrl = new URL(`/v1/database/${target.mod}/schema?version=9`, target.uri);
  let schema: { reducers?: { name: string; params?: { elements?: { name?: { some?: string };
    algebraic_type?: Record<string, unknown> }[] } }[]; typespace?: { types?: Record<string, unknown>[] } };
  try {
    schema = await withBrowserRequest(capture.page.request, async api => {
      const response = await api.get(schemaUrl.href, { timeout: 10_000 });
      return response.ok() ? response.json() : null;
    });
    if (!schema || typeof schema !== 'object') return null;
  } catch { return null; }
  // Use SDK codecs for scalar values, options, and unit enums. Records and arrays
  // need additional claim-location metadata; never report their fields absent.
  const argumentType = (raw: Record<string, unknown> | undefined, depth = 0):
    { tag: string; value?: unknown } | undefined => {
    if (!raw || depth > 8) return undefined;
    const tags = Object.keys(raw);
    if (tags.length !== 1) return undefined;
    if (typeof raw.Ref === 'number') return argumentType(schema.typespace?.types?.[raw.Ref], depth + 1);
    if (['String', 'Bool', 'I8', 'U8', 'I16', 'U16', 'I32', 'U32', 'I64', 'U64',
      'I128', 'U128', 'I256', 'U256', 'F32', 'F64'].includes(tags[0]!)) return { tag: tags[0]! };
    const product = raw.Product as { elements?: unknown[] } | undefined;
    if (Array.isArray(product?.elements) && product.elements.length === 0) {
      return { tag: 'Product', value: { elements: [] } };
    }
    const sum = raw.Sum as { variants?: { name?: { some?: string }; algebraic_type?: Record<string, unknown> }[] } | undefined;
    if (!Array.isArray(sum?.variants) || !sum.variants.length || sum.variants.length > 256) return undefined;
    const variants = sum.variants.map(variant => ({ name: variant.name?.some,
      algebraicType: argumentType(variant.algebraic_type, depth + 1) }));
    if (variants.some(variant => typeof variant.name !== 'string' || !variant.algebraicType)
      || new Set(variants.map(variant => variant.name)).size !== variants.length) return undefined;
    const optional = variants.length === 2 && variants[0]!.name === 'some' && variants[1]!.name === 'none'
      && variants[1]!.algebraicType!.tag === 'Product';
    if (!optional && variants.some(variant => variant.algebraicType!.tag !== 'Product')) return undefined;
    return { tag: 'Sum', value: { variants } };
  };
  const declarations = new Map<string, { names: string[]; read: ((reader: Reader) => unknown)[];
    write: ((writer: Writer, value: unknown) => void)[] }>();
  for (const reducer of schema.reducers ?? []) {
    const fields = reducer.params?.elements;
    if (!fields?.length || !fields.every(field => typeof field.name?.some === 'string')) continue;
    const types = fields.map(field => argumentType(field.algebraic_type));
    if (types.some(type => !type)) continue;
    declarations.set(reducer.name, {
      names: fields.map(field => field.name!.some!),
      read: types.map(type => capture.codec.AlgebraicType.makeDeserializer(type!)),
      write: types.map(type => capture.codec.AlgebraicType.makeSerializer(type!)),
    });
  }
  return declarations;
}

// A submit can contain several writes. This path captures each real reducer
// call; it does not infer which call creates an account from a later sign-in.
export async function startSpacetimeAuthWriteCapture(page: object,
  visit: (route: { url: string; reducer: string; flags: number; parameters: readonly string[] }, args: unknown[]) =>
    { value: unknown; shape: string; absentParameters?: string[] } | null,
  receipt: (value: AuthReceipt, changed: boolean) => void, onFailure: () => void) {
  const capture = captures.get(page);
  if (!capture) return null;
  if (capture.auth) throw new Error('Authentication request capture is already active');
  const declarations = await authDeclarations(capture), target = leasedSpacetimeTarget();
  const waiting: { socket: Socket; id: number; finish(result: string): void }[] = [];
  const pending: Promise<void>[] = [];
  let stopped = false, timer: ReturnType<typeof setTimeout> | undefined;
  const fail = () => { onFailure(); for (const item of waiting.splice(0)) item.finish('unknown'); };
  capture.auth = {
    fail,
    change(message, socket) {
      if (stopped) return null;
      const declaration = declarations?.get(message.value.reducer);
      if (!declaration || !leasedSocket(capture, socket, target, true, true)) {
        fail(); throw new Error('Unproved native signup write');
      }
      const reader = new capture.codec.BinaryReader(message.value.args);
      const args = declaration.read.map(read => read(reader));
      const encodeArgs = (values: unknown[]) => {
        const writer = new capture.codec.BinaryWriter(128);
        declaration.write.forEach((write, index) => write(writer, values[index]));
        return writer.getBuffer();
      };
      if (reader.remaining || !Buffer.from(encodeArgs(args)).equals(Buffer.from(message.value.args))) {
        fail(); throw new Error('Native write did not round trip');
      }
      const changed = visit({ url: socket.url, reducer: message.value.reducer,
        flags: message.value.flags, parameters: declaration.names }, args);
      const values = changed?.value ?? args;
      if (!Array.isArray(values) || values.length !== args.length) { fail(); throw new Error('Invalid native write patch'); }
      const encoded = encodeArgs(values), check = new capture.codec.BinaryReader(encoded);
      if (!isDeepStrictEqual(declaration.read.map(read => read(check)), values) || check.remaining) {
        fail(); throw new Error('Native write patch changed type');
      }
      const sent = { ...message, value: { ...message.value, args: encoded } };
      const bodySha256 = createHash('sha256').update(encode(capture.codec, capture.codec.ClientMessage, sent)).digest('hex');
      pending.push(new Promise<void>(resolve => waiting.push({ socket, id: message.value.requestId, finish(result) {
        if (!['Ok', 'OkEmpty', 'Err'].includes(result)) onFailure();
        else receipt({ shape: changed?.shape ?? 'captured', success: result !== 'Err',
          transport: 'spacetime-websocket', bodySha256,
          ...(changed?.absentParameters ? { absentParameters: changed.absentParameters } : {}) }, Boolean(changed));
        resolve();
      } })));
      return changed ? sent : null;
    },
  };
  capture.authResult = (socket, id, result) => {
    const index = waiting.findIndex(item => item.socket === socket && item.id === id);
    if (index >= 0) waiting.splice(index, 1)[0]!.finish(result);
  };
  capture.authProcedure = () => { if (!stopped) fail(); };
  return {
    stop: () => { stopped = true; },
    finish: async () => {
      timer = setTimeout(fail, 10_000);
      try {
        let consumed = 0;
        do {
          const batch = pending.slice(consumed);
          consumed = pending.length;
          await Promise.all(batch);
        } while (consumed < pending.length);
      } finally { clearTimeout(timer); }
    },
    dispose: () => {
      clearTimeout(timer); capture.auth = undefined; capture.authResult = undefined; capture.authProcedure = undefined;
      if (waiting.length) fail();
    },
  };
}

// Patch the app's own credential reducer call on its existing socket. The
// declared reducer parameters are the only authority for locating extra fields.
export async function startSpacetimeAuthPatch(page: object, username: string, password: string,
  patch: (args: unknown[], parameters: readonly { name: string }[], observedPassword?: string | null) =>
    { value: unknown; shape: string; absentParameters?: string[] } | null,
  onFailure: () => void, kind?: AuthKind, fieldsOnly = false) {
  const capture = captures.get(page);
  if (!capture) return null;
  if (capture.auth) throw new Error('Authentication request patch is already active');
  const target = leasedSpacetimeTarget();
  const declarations = await authDeclarations(capture);
  if (!declarations) return { receipt: async () => undefined, dispose: () => {} };
  const observed = kind ? capture.authWitnesses.filter(witness => witness.kind === kind) : [];
  const witnessValues = (witness: AuthWitness) => {
    const declaration = declarations.get(witness.item.call.reducer);
    if (!declaration) return null;
    const reader = new capture.codec.BinaryReader(witness.item.call.args);
    const values = declaration.read.map(read => read(reader));
    if (reader.remaining) return null;
    const writer = new capture.codec.BinaryWriter(128);
    declaration.write.forEach((write, index) => write(writer, values[index]));
    return Buffer.from(writer.getBuffer()).equals(Buffer.from(witness.item.call.args)) ? values : null;
  };
  type WitnessRoute = { reducer: string; userAt: number; credentialAt?: number; baseline: unknown[] };
  let witnessRoute: WitnessRoute | undefined;
  if (kind === 'signin' && observed.length === 2) {
    const positive = observed.find(witness => witness.user === username && witness.accepted && witness.usable);
    const negative = observed.find(witness => witness.user === username && !witness.accepted);
    if (positive && negative && positive.item.call.reducer === negative.item.call.reducer
      && positive.item.call.flags === negative.item.call.flags) {
      const first = witnessValues(positive), second = witnessValues(negative);
      const userAt = first?.findIndex(value => value === username) ?? -1;
      const differences = first?.map((value, index) => isDeepStrictEqual(value, second?.[index]) ? -1 : index)
        .filter(index => index >= 0) ?? [];
      if (first && second && first.length === second.length && userAt >= 0
        && first.filter(value => value === username).length === 1
        && second[userAt] === username && differences.length === 1 && differences[0] !== userAt) {
        witnessRoute = { reducer: positive.item.call.reducer, userAt,
          credentialAt: differences[0]!, baseline: second };
      }
    }
  }
  if (kind === 'signup' && fieldsOnly && observed.length === 1 && observed[0]!.usable && observed[0]!.item.committed) {
    const witness = observed[0]!;
    const values = witnessValues(witness);
    const userAt = values?.findIndex(value => value === witness.user) ?? -1;
    if (values && userAt >= 0 && values.filter(value => value === witness.user).length === 1) {
      witnessRoute = { reducer: witness.item.call.reducer, userAt, baseline: values };
    }
  }
  // A partial witness never authorizes the old raw-value fallback.
  const witnessUnproved = kind && capture.authUnproved.has(kind) || observed.length > 0 && !witnessRoute;
  let matched = false, settled = false, failed = false, timer: ReturnType<typeof setTimeout> | undefined;
  let finish!: (value: AuthReceipt | undefined) => void;
  const done = new Promise<AuthReceipt | undefined>(resolve => {
    finish = value => { if (settled) return; settled = true; clearTimeout(timer); resolve(value); };
  });
  // Invalidation can follow the first receipt while the form is still active.
  const fail = () => { failed = true; onFailure(); finish(undefined); };
  capture.auth = {
    change(message, socket) {
      const declaration = declarations.get(message.value.reducer);
      if (!declaration) return null;
      const reader = new capture.codec.BinaryReader(message.value.args);
      const values = declaration.read.map(read => read(reader));
      if (reader.remaining || witnessUnproved) return null;
      let observedPassword: string | null | undefined;
      if (witnessRoute) {
        if (message.value.reducer !== witnessRoute.reducer || values.length !== witnessRoute.baseline.length
          || values[witnessRoute.userAt] !== username || values.filter(value => value === username).length !== 1) return null;
        if (kind === 'signup') {
          observedPassword = null;
        } else if (values.some((value, index) => index !== witnessRoute!.userAt
          && !isDeepStrictEqual(value, witnessRoute!.baseline[index]))) return null;
        else {
          const credential = values[witnessRoute.credentialAt!];
          if (typeof credential !== 'string' || credential === username) return null;
          observedPassword = credential;
        }
      } else if (values.filter(value => value === username).length !== 1
        || values.filter(value => value === password).length !== 1) return null;
      if (matched) { fail(); throw new Error('Ambiguous credential call'); }
      if (!leasedSocket(capture, socket, target, true, true)) return null;
      const writer = new capture.codec.BinaryWriter(128);
      declaration.write.forEach((write, index) => write(writer, values[index]));
      if (!Buffer.from(writer.getBuffer()).equals(Buffer.from(message.value.args))
        || socket.authPending || socket.invalid) { fail(); throw new Error('Ambiguous credential call'); }
      const changed = patch(values, declaration.names.map(name => ({ name })), observedPassword);
      if (!changed) return null;
      const args = changed.value;
      if (!Array.isArray(args) || args.length !== values.length) {
        fail(); throw new Error('Unsupported credential arguments');
      }
      const encoded = new capture.codec.BinaryWriter(128);
      declaration.write.forEach((write, index) => write(encoded, args[index]));
      const roundTrip = new capture.codec.BinaryReader(encoded.getBuffer());
      if (!isDeepStrictEqual(declaration.read.map(read => read(roundTrip)), args) || roundTrip.remaining) {
        fail(); throw new Error('Credential arguments changed type');
      }
      const sent = { ...message, value: { ...message.value, args: encoded.getBuffer() } };
      const bodySha256 = createHash('sha256').update(encode(capture.codec, capture.codec.ClientMessage, sent)).digest('hex');
      matched = true;
      socket.authPending = { id: message.value.requestId, finish(result) {
        socket.authPending = undefined;
        if (!['Ok', 'OkEmpty', 'Err'].includes(result) || socket.invalid) { fail(); return; }
        finish({ shape: changed.shape, success: result !== 'Err', transport: 'spacetime-websocket',
          bodySha256, ...(changed.absentParameters ? { absentParameters: changed.absentParameters } : {}) });
      } };
      timer = setTimeout(() => socket.authPending?.finish('unknown'), 30_000);
      return sent;
    },
    fail,
  };
  return {
    receipt: async () => { const value = matched ? await done : undefined; return failed ? undefined : value; },
    dispose: () => {
      capture.auth = undefined;
      finish(undefined);
      for (const witness of capture.authWitnesses) {
        for (const socket of capture.sockets) socket.calls = socket.calls.filter(item => item !== witness.item);
      }
      capture.authWitnesses = [];
      capture.authUnproved.clear();
      witnessRoute?.baseline.fill(undefined);
    },
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
  const schema = await withBrowserRequest(capture.page.request, async api => {
    const response = await api.get(url.href, { timeout: 10000 });
    return response.ok() ? response.json() : null;
  });
  if (!schema) return null;
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
