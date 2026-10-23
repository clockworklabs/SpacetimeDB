import { afterEach, describe, expect, test, vi } from 'vitest';
import {
  AlgebraicType,
  type AlgebraicTypeType,
} from '../src/lib/algebraic_type';
import BinaryReader from '../src/lib/binary_reader';
import BinaryWriter from '../src/lib/binary_writer';
import {
  canCompileFunctions,
  setCanCompileFunctionsForTesting,
} from '../src/lib/runtime_codegen';

// When `Function` is unavailable (a strict CSP, Cloudflare Workers), the
// (de)serializers are built from closures instead. These tests check that both
// paths agree byte for byte.
//
// (De)serializers are cached per type object, so each case builds a fresh type
// for each path.

type Case = {
  name: string;
  type: () => { ty: AlgebraicTypeType; typespace?: { types: any[] } };
  values: any[];
};

const unit = () => AlgebraicType.Product({ elements: [] });
const option = (ty: AlgebraicTypeType) =>
  AlgebraicType.Sum({
    variants: [
      { name: 'some', algebraicType: ty },
      { name: 'none', algebraicType: unit() },
    ],
  });

const cases: Case[] = [
  {
    name: 'fixed-size product',
    type: () => ({
      ty: AlgebraicType.Product({
        elements: [
          { name: 'b', algebraicType: AlgebraicType.Bool },
          { name: 'i8', algebraicType: AlgebraicType.I8 },
          { name: 'u8', algebraicType: AlgebraicType.U8 },
          { name: 'i16', algebraicType: AlgebraicType.I16 },
          { name: 'u16', algebraicType: AlgebraicType.U16 },
          { name: 'i32', algebraicType: AlgebraicType.I32 },
          { name: 'u32', algebraicType: AlgebraicType.U32 },
          { name: 'i64', algebraicType: AlgebraicType.I64 },
          { name: 'u64', algebraicType: AlgebraicType.U64 },
          { name: 'i128', algebraicType: AlgebraicType.I128 },
          { name: 'u128', algebraicType: AlgebraicType.U128 },
          { name: 'i256', algebraicType: AlgebraicType.I256 },
          { name: 'u256', algebraicType: AlgebraicType.U256 },
          { name: 'f32', algebraicType: AlgebraicType.F32 },
          { name: 'f64', algebraicType: AlgebraicType.F64 },
        ],
      }),
    }),
    values: [
      {
        b: true,
        i8: -8,
        u8: 8,
        i16: -16,
        u16: 16,
        i32: -32,
        u32: 32,
        i64: -64n,
        u64: 64n,
        i128: -(1n << 100n),
        u128: 1n << 100n,
        i256: -(1n << 200n),
        u256: 1n << 200n,
        f32: 1.5,
        f64: Math.PI,
      },
      {
        b: false,
        i8: 0,
        u8: 255,
        i16: 0,
        u16: 65535,
        i32: 0,
        u32: 4294967295,
        i64: 0n,
        u64: (1n << 64n) - 1n,
        i128: 0n,
        u128: 0n,
        i256: 0n,
        u256: (1n << 256n) - 1n,
        f32: -0.25,
        f64: -1e300,
      },
    ],
  },
  {
    name: 'product with strings, arrays and nested products',
    type: () => ({
      ty: AlgebraicType.Product({
        elements: [
          { name: 'name', algebraicType: AlgebraicType.String },
          {
            name: 'tags',
            algebraicType: AlgebraicType.Array(AlgebraicType.String),
          },
          {
            name: 'bytes',
            algebraicType: AlgebraicType.Array(AlgebraicType.U8),
          },
          {
            name: 'pos',
            algebraicType: AlgebraicType.Product({
              elements: [
                { name: 'x', algebraicType: AlgebraicType.F32 },
                { name: 'y', algebraicType: AlgebraicType.F32 },
              ],
            }),
          },
          { name: 'nick', algebraicType: option(AlgebraicType.String) },
        ],
      }),
    }),
    values: [
      {
        name: 'alice',
        tags: ['a', 'b'],
        bytes: new Uint8Array([1, 2, 3]),
        pos: { x: 1.5, y: -2.5 },
        nick: 'al',
      },
      {
        name: '',
        tags: [],
        bytes: new Uint8Array([]),
        pos: { x: 0, y: 0 },
        nick: undefined,
      },
    ],
  },
  {
    name: 'sum',
    type: () => ({
      ty: AlgebraicType.Sum({
        variants: [
          { name: 'Empty', algebraicType: unit() },
          { name: 'Count', algebraicType: AlgebraicType.U32 },
          { name: 'Label', algebraicType: AlgebraicType.String },
        ],
      }),
    }),
    values: [
      { tag: 'Empty', value: {} },
      { tag: 'Count', value: 7 },
      { tag: 'Label', value: 'hi' },
    ],
  },
  {
    name: 'recursive product',
    type: () => ({
      ty: AlgebraicType.Ref(0),
      typespace: {
        types: [
          AlgebraicType.Product({
            elements: [
              { name: 'value', algebraicType: AlgebraicType.I32 },
              { name: 'next', algebraicType: option(AlgebraicType.Ref(0)) },
            ],
          }),
        ],
      },
    }),
    values: [
      { value: 1, next: { value: 2, next: { value: 3, next: undefined } } },
    ],
  },
  {
    name: 'recursive sum',
    type: () => ({
      ty: AlgebraicType.Ref(0),
      typespace: {
        types: [
          AlgebraicType.Sum({
            variants: [
              { name: 'Leaf', algebraicType: AlgebraicType.U32 },
              {
                name: 'Node',
                algebraicType: AlgebraicType.Product({
                  elements: [
                    { name: 'left', algebraicType: AlgebraicType.Ref(0) },
                    { name: 'right', algebraicType: AlgebraicType.Ref(0) },
                  ],
                }),
              },
            ],
          }),
        ],
      },
    }),
    values: [
      {
        tag: 'Node',
        value: {
          left: { tag: 'Leaf', value: 1 },
          right: {
            tag: 'Node',
            value: {
              left: { tag: 'Leaf', value: 2 },
              right: { tag: 'Leaf', value: 3 },
            },
          },
        },
      },
    ],
  },
];

function serializeAll(compile: boolean, c: Case) {
  setCanCompileFunctionsForTesting(compile);
  const { ty, typespace } = c.type();
  const serialize = AlgebraicType.makeSerializer(ty, typespace);
  const deserialize = AlgebraicType.makeDeserializer(ty, typespace);
  return c.values.map(value => {
    const writer = new BinaryWriter(8);
    serialize(writer, value);
    const bytes = writer.getBuffer().slice();
    return { bytes, roundTripped: deserialize(new BinaryReader(bytes)) };
  });
}

describe('serialization without Function', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    setCanCompileFunctionsForTesting(undefined);
  });

  test('detects when Function is forbidden', () => {
    vi.stubGlobal('Function', () => {
      throw new EvalError('Code generation from strings disallowed');
    });
    setCanCompileFunctionsForTesting(undefined);
    expect(canCompileFunctions()).toBe(false);
  });

  test.each(cases)('$name matches the Function path', c => {
    const compiled = serializeAll(true, c);

    // Forbid `Function` as a strict CSP would.
    vi.stubGlobal('Function', () => {
      throw new EvalError('Code generation from strings disallowed');
    });
    const closures = serializeAll(false, c);

    for (let i = 0; i < c.values.length; i++) {
      expect(closures[i].bytes).toEqual(compiled[i].bytes);
      expect(closures[i].roundTripped).toEqual(c.values[i]);
      expect(compiled[i].roundTripped).toEqual(c.values[i]);
    }
  });

  test('throws on an unknown sum tag', () => {
    setCanCompileFunctionsForTesting(false);
    const { ty } = cases.find(c => c.name === 'sum')!.type();
    const serialize = AlgebraicType.makeSerializer(ty);
    expect(() =>
      serialize(new BinaryWriter(8), { tag: 'Nope', value: 0 })
    ).toThrow('Could not serialize sum type; unknown tag Nope');
  });
});
