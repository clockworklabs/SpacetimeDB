// Observe native requests; never infer a session argument from its name or schema.
const sessions = new WeakMap<object, {
  calls: { origin: string; args: Record<string, unknown>; socket?: object; requestId?: number; path?: string; confirmed?: boolean }[];
  bearer: Map<string, string>;
}>();

export function recordConvexSession(page: object, socketUrl: string, payload: string | Buffer, socket?: object): void {
  const url = new URL(socketUrl);
  if (!/^\/api\/[^/]+\/sync$/.test(url.pathname)) return;
  url.protocol = url.protocol === 'wss:' ? 'https:' : 'http:';
  let frame;
  try { frame = JSON.parse(String(payload)); } catch { return; }
  if (!frame || typeof frame !== 'object' || Array.isArray(frame)) return;
  const state = sessions.get(page) ?? { calls: [], bearer: new Map<string, string>() };
  sessions.set(page, state);
  if (frame.type === 'Authenticate' && frame.tokenType === 'User' && typeof frame.value === 'string') {
    state.bearer.set(url.origin, frame.value);
  }
  if (frame.type === 'Authenticate' && frame.tokenType === 'None') state.bearer.delete(url.origin);
  const calls = frame.type === 'ModifyQuerySet' ? frame.modifications
    : ['Mutation', 'Action'].includes(frame.type) ? [frame] : [];
  if (Array.isArray(calls)) for (const call of calls) {
    const args = call?.args?.length === 1 ? call.args[0] : null;
    if (args && typeof args === 'object' && !Array.isArray(args)) state.calls.push({ origin: url.origin, args,
      ...(socket && frame.type === 'Mutation' && Number.isSafeInteger(frame.requestId)
        && typeof frame.udfPath === 'string' && !frame.componentPath
        ? { socket, requestId: frame.requestId, path: frame.udfPath } : {}) });
  }
  if (state.calls.length > 200) state.calls.splice(0, state.calls.length - 200);
}

export function recordConvexMutationResult(page: object, socket: object, payload: string | Buffer): void {
  let frame;
  try { frame = JSON.parse(String(payload)); } catch { return; }
  if (frame?.type !== 'MutationResponse') return;
  const matches = sessions.get(page)?.calls.filter(call => call.socket === socket && call.requestId === frame.requestId) ?? [];
  if (matches.length === 1) matches[0]!.confirmed = frame.success === true;
}

// Only one confirmed mutation and one exact argument value may supply a replay.
// Keep the captured request unchanged; no substring or field-name guessing.
export function capturedConvexMutation(page: object, find: string, replacement: string) {
  const occurrences = (value: unknown): number => value === find ? 1 : value && typeof value === 'object'
    ? Object.values(value).reduce<number>((sum, child) => sum + occurrences(child), 0) : 0;
  const matches = sessions.get(page)?.calls.filter(call => call.path && occurrences(call.args)) ?? [];
  if (matches.length !== 1 || !matches[0]!.confirmed || occurrences(matches[0]!.args) !== 1) return null;
  const call = matches[0]!;
  // The Functions API helper uses plain JSON, not Convex's extended wire values.
  let encodedValue = false;
  const args = JSON.parse(JSON.stringify(call.args, (key, value) => {
    if (key.startsWith('$')) encodedValue = true;
    return value === find ? replacement : value;
  }));
  if (encodedValue) return null;
  return { origin: call.origin, path: call.path!, args: args as Record<string, unknown> };
}

export function convexSessionBinding(page: object, url: string, token: string, applicationOrigin?: string) {
  const state = sessions.get(page), origin = new URL(url).origin;
  // Apps may proxy the native sync protocol through the runner's application URL.
  // Accept that explicit origin only; unrelated sockets cannot supply credentials.
  const origins = new Set([origin, ...(applicationOrigin ? [applicationOrigin] : [])]);
  const records = (state?.calls ?? []).filter(record => origins.has(record.origin));
  const fields = new Set(records.flatMap(record => Object.entries(record.args ?? {})
    .filter(([, value]) => value === token).map(([key]) => key)));
  const bearer = [...origins].some(value => state?.bearer.get(value) === token);
  // A query may also echo the native token. That does not make its argument
  // part of every other function's interface.
  if (bearer) return { argument: undefined, bearer: true };
  if (fields.size !== 1) return null;
  return { argument: [...fields][0], bearer: false };
}
