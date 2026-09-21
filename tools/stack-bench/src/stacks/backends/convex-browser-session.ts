// Observe native requests; never infer a session argument from its name or schema.
const sessions = new WeakMap<object, {
  calls: { origin: string; args: Record<string, unknown> }[];
  bearer: Map<string, string>;
}>();

export function recordConvexSession(page: object, socketUrl: string, payload: string | Buffer): void {
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
    if (args && typeof args === 'object' && !Array.isArray(args)) state.calls.push({ origin: url.origin, args });
  }
  if (state.calls.length > 200) state.calls.splice(0, state.calls.length - 200);
}

export function convexSessionBinding(page: object, url: string, token: string) {
  const state = sessions.get(page), origin = new URL(url).origin;
  const records = (state?.calls ?? []).filter(record => record.origin === origin);
  const fields = new Set(records.flatMap(record => Object.entries(record.args ?? {})
    .filter(([, value]) => value === token).map(([key]) => key)));
  const bearer = state?.bearer.get(origin) === token;
  // A query may also echo the native token. That does not make its argument
  // part of every other function's interface.
  if (bearer) return { argument: undefined, bearer: true };
  if (fields.size !== 1) return null;
  return { argument: [...fields][0], bearer: false };
}
