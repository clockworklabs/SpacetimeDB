import type {
  AlgebraicTypeType,
  ProductTypeType,
  SumTypeType,
} from './algebraic_type';

export type Ref = { tag: 'Ref'; value: number };
export type Sum = { tag: 'Sum'; value: SumTypeType };
export type Product = { tag: 'Product'; value: ProductTypeType };
export type Array = { tag: 'Array'; value: AlgebraicTypeType };
export type String = { tag: 'String' };
export type Bool = { tag: 'Bool' };
export type I8 = { tag: 'I8' };
export type U8 = { tag: 'U8' };
export type I16 = { tag: 'I16' };
export type U16 = { tag: 'U16' };
export type I32 = { tag: 'I32' };
export type U32 = { tag: 'U32' };
export type I64 = { tag: 'I64' };
export type U64 = { tag: 'U64' };
export type I128 = { tag: 'I128' };
export type U128 = { tag: 'U128' };
export type I256 = { tag: 'I256' };
export type U256 = { tag: 'U256' };
export type F32 = { tag: 'F32' };
export type F64 = { tag: 'F64' };
