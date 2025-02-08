export class EventEmitter<Callback extends Function = Function> {
  #events: Map<string, Set<Callback>> = new Map();

  on(event: string, callback: Callback): void {
    let callbacks = this.#events.get(event);
    if (!callbacks) {
      callbacks = new Set();
      this.#events.set(event, callbacks);
    }
    callbacks.add(callback);
  }

  off(event: string, callback: Callback): void {
    let callbacks = this.#events.get(event);
    if (!callbacks) {
      return;
    }
    callbacks.delete(callback);
  }

  emit(event: string, ...args: any[]): void {
    let callbacks = this.#events.get(event);
    if (!callbacks) {
      return;
    }

    for (let callback of callbacks) {
      callback(...args);
    }
  }
}
