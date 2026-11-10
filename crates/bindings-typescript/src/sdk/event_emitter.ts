import { stdbLogger } from './logger.ts';

// eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
export class EventEmitter<Key, Callback extends Function = Function> {
  #events: Map<Key, Set<Callback>> = new Map();

  on(event: Key, callback: Callback): void {
    let callbacks = this.#events.get(event);
    if (!callbacks) {
      callbacks = new Set();
      this.#events.set(event, callbacks);
    }
    callbacks.add(callback);
  }

  off(event: Key, callback: Callback): void {
    const callbacks = this.#events.get(event);
    if (!callbacks) {
      return;
    }
    callbacks.delete(callback);
  }

  emit(event: Key, ...args: any[]): void {
    const callbacks = this.#events.get(event);
    if (!callbacks) {
      return;
    }

    // Like the C# SDK, a throwing callback is logged and does not stop the
    // others or escape into the WebSocket listener, which would crash Node.
    for (const callback of callbacks) {
      try {
        callback(...args);
      } catch (e) {
        stdbLogger('error', 'A callback threw', e);
      }
    }
  }
}
