import { AlgebraicType } from './algebraic_type';

export type OptionAlgebraicType = {
  tag: 'Sum';
  value: {
    variants: [
      { name: 'some'; algebraicType: AlgebraicType },
      {
        name: 'none';
        algebraicType: { tag: 'Product'; value: { elements: [] } };
      },
    ];
  };
};

export const Option: {
  getAlgebraicType(innerType: AlgebraicType): OptionAlgebraicType;
} = {
  getAlgebraicType(innerType: AlgebraicType): OptionAlgebraicType {
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
