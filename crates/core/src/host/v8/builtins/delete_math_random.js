delete Math.random;
Object.defineProperty(Math, 'random', {
  writable: true,
  enumerable: false,
  configurable: true,
  get() {
    throw new TypeError(
      'Math.random is not available in SpacetimeDB modules. Use ctx.random instead.'
    );
  },
});
