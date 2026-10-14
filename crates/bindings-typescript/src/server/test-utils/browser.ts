export function createModuleTestHarness(): never {
  throw new Error(
    'spacetimedb/server/test-utils is only supported in Node tests until the Wasm datastore adapter is implemented.'
  );
}

export const TestAuth = {
  internal(): never {
    throw new Error(
      'spacetimedb/server/test-utils is only supported in Node tests until the Wasm datastore adapter is implemented.'
    );
  },
  fromJwtPayload(): never {
    throw new Error(
      'spacetimedb/server/test-utils is only supported in Node tests until the Wasm datastore adapter is implemented.'
    );
  },
};
