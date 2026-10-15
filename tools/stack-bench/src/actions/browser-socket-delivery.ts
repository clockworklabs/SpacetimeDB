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
  const gate = gates.get(context)?.(client, server);
  return {
    server,
    send(data: string | Buffer) { gate?.sent(data); server.send(data); },
    receive(data: string | Buffer) { return gate?.received(data) ?? true; },
    closed() { gate?.closed(); },
  };
}
