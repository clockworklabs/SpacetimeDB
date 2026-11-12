import { describe, expect, it } from 'vitest';
import { spacetimedbModuleTestPlugin } from '../src/server/test-utils/vitest';

const resolve = (id: string) => {
  const hook = spacetimedbModuleTestPlugin().resolveId as (
    id: string
  ) => string | null;
  return hook(id);
};

describe('spacetimedbModuleTestPlugin', () => {
  it('stubs every spacetime:sys version, including ones not yet released', () => {
    const stub = resolve('spacetime:sys@2.0');
    expect(stub).toBeTruthy();
    for (const version of ['2.1', '2.2', '2.3']) {
      expect(resolve(`spacetime:sys@${version}`)).toBe(stub);
    }
  });

  it('leaves other imports alone', () => {
    expect(resolve('spacetimedb')).toBeNull();
    expect(resolve('./spacetime:sys@2.0')).toBeNull();
  });
});
