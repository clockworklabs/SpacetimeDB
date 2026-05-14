if (!Symbol.dispose) {
  Object.defineProperty(Symbol, 'dispose', {
    value: Symbol.for('Symbol.dispose'),
  });
}

if (!Set.prototype.isSubsetOf) {
  Object.defineProperty(Set.prototype, 'isSubsetOf', {
    value: function isSubsetOf<T>(this: Set<T>, other: Set<T>) {
      return [...this].every(value => other.has(value));
    },
  });
}
