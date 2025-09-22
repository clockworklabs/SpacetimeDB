export async function resolveWS(): Promise<typeof WebSocket> {
  if ('WebSocket' in globalThis) {
    return WebSocket as unknown as typeof WebSocket;
  }
  try {
    const { WebSocket: UndiciWS } = await import('undici');
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
