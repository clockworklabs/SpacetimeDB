import { AlgebraicType } from './algebraic_type';

export type OptionAlgebraicType<T extends AlgebraicType = AlgebraicType> = {
  tag: 'Sum';
  value: {
    variants: [
      { name: 'some'; algebraicType: T },
      {
        name: 'none';
        algebraicType: { tag: 'Product'; value: { elements: [] } };
      },
    ];
  };
};

export const Option: {
  getAlgebraicType<T extends AlgebraicType = AlgebraicType>(
    innerType: T
  ): OptionAlgebraicType<T>;
} = {
  getAlgebraicType<T extends AlgebraicType = AlgebraicType>(
    innerType: T
  ): OptionAlgebraicType<T> {
    return AlgebraicType.Sum({
      variants: [
        { name: 'some', algebraicType: innerType },
        {
          name: 'none',
          algebraicType: AlgebraicType.Product({ elements: [] }),
        },
      ],
    });
  },
};
