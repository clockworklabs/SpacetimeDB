import { describe, it, expect } from 'vitest';
import { AlgebraicType, t } from '../src/index';

describe('TypeBuilder', () => {
  it('builds the correct algebraic type for a point', () => {
    const point = t.object({
      x: t.f64(),
      y: t.f64(),
      z: t.f64(),
    });
    expect(point.algebraicType).toEqual({
      tag: 'Product',
      value: {
        elements: [
          { name: 'x', algebraicType: AlgebraicType.F64 },
          { name: 'y', algebraicType: AlgebraicType.F64 },
          { name: 'z', algebraicType: AlgebraicType.F64 },
        ],
      },
    });
  });
});
