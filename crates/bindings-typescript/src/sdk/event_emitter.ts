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

  /** @internal Clear callbacks when an explicitly managed generation ends. */
  clear(): void {
    for (const callbacks of this.#events.values()) callbacks.clear();
    this.#events.clear();
  }

  emit(event: Key, ...args: any[]): void {
    const callbacks = this.#events.get(event);
    if (!callbacks) {
      return;
    }

    for (const callback of callbacks) {
      callback(...args);
    }
  }
}
