export class EventEmitter {
  #events: Map<string, Set<Function>> = new Map();

  on(event: string, callback: Function): void {
    let callbacks = this.#events.get(event);
    if (!callbacks) {
      callbacks = new Set();
      this.#events.set(event, callbacks);
    }
    callbacks.add(callback);
  }

  off(event: string, callback: Function): void {
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
