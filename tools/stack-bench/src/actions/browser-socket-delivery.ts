import type { BrowserContext, WebSocketRoute } from 'playwright';

// Native capture owns parsing and receipts. Response loss controls delivery on
// that same connection, before a reply can satisfy any receipt matcher.
interface DeliveryGate {
  sent(data: string | Buffer): void;
  received(data: string | Buffer): boolean;
  closed(): void;
}
const gates = new WeakMap<BrowserContext,
  (client: WebSocketRoute, server: WebSocketRoute) => DeliveryGate>();

export function installSocketDeliveryGate(context: BrowserContext,
  gate: (client: WebSocketRoute, server: WebSocketRoute) => DeliveryGate): void {
  if (gates.has(context)) throw new Error('Socket delivery gate already installed');
  gates.set(context, gate);
  context.once('close', () => gates.delete(context));
}

export function connectBrowserSocket(context: BrowserContext, client: WebSocketRoute) {
  const server = client.connectToServer();
  // Found on each frame, so a socket opened before the gate is installed is still gated.
  let gate: DeliveryGate | undefined;
  const current = () => gate ??= gates.get(context)?.(client, server);
  return {
    server,
    send(data: string | Buffer) { current()?.sent(data); server.send(data); },
    receive(data: string | Buffer) { return current()?.received(data) ?? true; },
    closed() { gate?.closed(); },
  };
}
