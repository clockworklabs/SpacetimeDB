export async function resolveWS(): Promise<typeof WebSocket> {
  // Browser or Node >= 22 (or any env that exposes global WebSocket)
  if (typeof (globalThis as any).WebSocket !== 'undefined') {
    return (globalThis as any).WebSocket as typeof WebSocket;
  }

  // Node without a global WebSocket: lazily load undici's polyfill.
  // Use an unstatable dynamic import so bundlers don't prebundle it.
  const dynamicImport = new Function('m', 'return import(m)') as (
    m: string
  ) => Promise<any>;

  try {
    const { WebSocket: UndiciWS } = await dynamicImport('undici');
    return UndiciWS as unknown as typeof WebSocket;
  } catch (err) {
    console.warn(
      '[spacetimedb-sdk] No global WebSocket found. ' +
        'On Node 18–21, please install `undici` (npm install undici) ' +
        'to enable WebSocket support.'
    );
    throw err;
  }
}
