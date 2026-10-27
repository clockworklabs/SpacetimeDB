import { describe, expect, it } from 'vitest';
import { t } from '../src/lib/type_builders';

describe('an enum with a variant named `create`', () => {
  it('builds, whatever the variant order', () => {
    const Action = t.enum('Action', ['create', 'update']);
    expect(Action.algebraicType.value.variants.map(v => v.name)).toEqual([
      'create',
      'update',
    ]);
  });

  it('keeps the other variant constructors working', () => {
    const Op = t.enum('Op', { create: t.unit(), set: t.string() });
    expect(Op.set('x')).toEqual({ tag: 'set', value: 'x' });
  });
});
