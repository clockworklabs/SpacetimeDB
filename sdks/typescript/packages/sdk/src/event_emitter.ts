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
    let callbacks = this.#events.get(event);
    if (!callbacks) {
      return;
    }
    callbacks.delete(callback);
  }

  emit(event: Key, ...args: any[]): void {
    let callbacks = this.#events.get(event);
    if (!callbacks) {
      return;
    }

    for (let callback of callbacks) {
      callback(...args);
    }
  }
}
