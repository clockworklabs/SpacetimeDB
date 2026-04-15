import { describe, expect, it, assertType } from 'vitest';
import { t, type InferTypeOfRow } from '../src/lib/type_builders';

const rowOptionOptional = {
  foo: t.string().optional().optional(),
};

type RowOptionOptional = InferTypeOfRow<typeof rowOptionOptional>;

const omitted: RowOptionOptional = {};
const none: RowOptionOptional = {
  foo: undefined,
};
const some: RowOptionOptional = {
  foo: 'hello',
};

void omitted;
void none;
void some;

describe('Type builder optional row inference', () => {
  it('allows omitted option-valued fields', () => {
    assertType<RowOptionOptional>(omitted);
    assertType<RowOptionOptional>(none);
    assertType<RowOptionOptional>(some);
    expect(true).toBe(true);
  });
});
