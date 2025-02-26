export default class OperationsMap<K, V> {
  #items: { key: K; value: V }[] = [];

  #isEqual(a: K, b: K): boolean {
    if (a && typeof a === 'object' && 'isEqual' in a) {
      return (a as any).isEqual(b);
    }
    return a === b;
  }

  set(key: K, value: V): void {
    const existingIndex = this.#items.findIndex(({ key: k }) =>
      this.#isEqual(k, key)
    );
    if (existingIndex > -1) {
      this.#items[existingIndex].value = value;
    } else {
      this.#items.push({ key, value });
    }
  }

  get(key: K): V | undefined {
    const item = this.#items.find(({ key: k }) => this.#isEqual(k, key));
    return item ? item.value : undefined;
  }

  delete(key: K): boolean {
    const existingIndex = this.#items.findIndex(({ key: k }) =>
      this.#isEqual(k, key)
    );
    if (existingIndex > -1) {
      this.#items.splice(existingIndex, 1);
      return true;
    }
    return false;
  }

  has(key: K): boolean {
    return this.#items.some(({ key: k }) => this.#isEqual(k, key));
  }

  values(): Array<V> {
    return this.#items.map(i => i.value);
  }

  entries(): Array<{ key: K; value: V }> {
    return this.#items;
  }

  [Symbol.iterator](): Iterator<{ key: K; value: V }> {
    let index = 0;
    const items = this.#items;
    return {
      next(): IteratorResult<{ key: K; value: V }> {
        if (index < items.length) {
          return { value: items[index++], done: false };
        } else {
          return { value: null, done: true };
        }
      },
    };
  }
}
