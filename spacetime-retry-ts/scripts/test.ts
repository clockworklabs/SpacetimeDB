import * as assert from 'node:assert/strict';
import type { AlgebraicType } from 'spacetimedb';
import type { TypeBuilder } from 'spacetimedb/server';
import {
  makeRetryDispatch,
  retryFailed,
  retryHandler,
  retryOk,
} from '../src/kit.ts';

const fakeBuilder = <T>(): TypeBuilder<T, AlgebraicType> =>
  ({}) as unknown as TypeBuilder<T, AlgebraicType>;

const calls: string[] = [];
const handlers = {
  noArgs: retryHandler(fakeBuilder<Record<never, never>>(), () => {
    calls.push('noArgs');
    return retryOk();
  }),
  withArgs: retryHandler(
    fakeBuilder<{ value: string }>(),
    (_ctx, args: { value: string }) => {
      calls.push(args.value);
      return retryFailed('try again');
    }
  ),
};

assert.deepEqual(Object.keys(handlers), ['noArgs', 'withArgs']);
const dispatch = makeRetryDispatch(handlers);
assert.deepEqual(dispatch({}, { tag: 'noArgs' }), { ok: true });
assert.deepEqual(
  dispatch({}, { tag: 'withArgs', value: { value: 'payload' } }),
  {
    ok: false,
    error: 'try again',
  }
);
assert.deepEqual(calls, ['noArgs', 'payload']);
assert.throws(
  () => dispatch({}, { tag: 'missing' as keyof typeof handlers }),
  /unknown retry handler/
);

console.log('retry tests passed');
