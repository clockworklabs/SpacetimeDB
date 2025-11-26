---
title: BSATN Data Format
slug: /bsatn
---

# Binary SATN Format (BSATN)

The Spacetime Algebraic Type Notation binary (BSATN) format defines
how Spacetime `AlgebraicValue`s and friends are encoded as byte strings.

Algebraic values and product values are BSATN-encoded for e.g.,
module-host communication and for storing row data in the database.

## Notes on notation

In this reference, we give a formal definition of the format.
To do this, we use inductive definitions, and define the following notation:

- `bsatn(x)` denotes a function converting some value `x` to a list of bytes.
- `a: B` means that `a` is of type `B`.
- `Foo(x)` denotes extracting `x` out of some variant or type `Foo`.
- `a ++ b` denotes concatenating two byte lists `a` and `b`.
- `bsatn(A) = bsatn(B) | ... | bsatn(Z)` where `B` to `Z` are variants of `A`
  means that `bsatn(A)` is defined as e.g.,
  `bsatn(B)`, `bsatn(C)`, .., `bsatn(Z)` depending on what variant of `A` it was.
- `[]` denotes the empty list of bytes.

## Values

### At a glance

| Type                                | Description                                          |
| ----------------------------------- | ---------------------------------------------------- |
| [`AlgebraicValue`](#algebraicvalue) | A value of any type.                                 |
| [`SumValue`](#sumvalue)             | A value of a sum type, i.e. an enum or tagged union. |
| [`ProductValue`](#productvalue)     | A value of a product type, i.e. a struct or tuple.   |

### `AlgebraicValue`

The BSATN encoding of an `AlgebraicValue` defers to the encoding of each variant:

```fsharp
bsatn(AlgebraicValue)
    = bsatn(SumValue)
    | bsatn(ProductValue)
    | bsatn(ArrayValue)
    | bsatn(String)
    | bsatn(Bool)
    | bsatn(U8) | bsatn(U16) | bsatn(U32) | bsatn(U64) | bsatn(U128) | bsatn(U256)
    | bsatn(I8) | bsatn(I16) | bsatn(I32) | bsatn(I64) | bsatn(I128) | bsatn(I256)
    | bsatn(F32) | bsatn(F64)
```

Algebraic values include sums, products, arrays, strings, and primitives types.
The primitive types include booleans, unsigned and signed integers up to 256-bits, and floats, both single and double precision.

### `SumValue`

An instance of a sum type, i.e. an enum or tagged union.
`SumValue`s are binary-encoded as `bsatn(tag) ++ bsatn(variant_data)`
where `tag: u8` is an index into the `SumType.variants`
array of the value's `SumType`,
and where `variant_data` is the data of the variant.
For variants holding no data, i.e., of some zero sized type,
`bsatn(variant_data) = []`.

### `ProductValue`

An instance of a product type, i.e. a struct or tuple.
`ProductValue`s are binary encoded as:

```fsharp
bsatn(elems) = bsatn(elem_0) ++ .. ++ bsatn(elem_n)
```

Field names are not encoded.

### `ArrayValue`

The encoding of an `ArrayValue` is:

```
bsatn(ArrayValue(a))
    = bsatn(len(a) as u32)
   ++ bsatn(normalize(a)_0)
   ++ ..
   ++ bsatn(normalize(a)_n)
```

where `normalize(a)` for `a: ArrayValue` converts `a` to a list of `AlgebraicValue`s.

### Strings

For strings, the encoding is defined as:

```fsharp
bsatn(String(s)) = bsatn(len(s) as u32) ++ bsatn(utf8_to_bytes(s))
```

That is, the BSATN encoding is the concatenation of

- the bsatn of the string's length as a `u32` integer byte
- the utf8 representation of the string as a byte array

### Primitives

For the primitive variants of `AlgebraicValue`, the BSATN encodings are:s

```fsharp
bsatn(Bool(false)) = [0]
bsatn(Bool(true)) = [1]
bsatn(U8(x)) = [x]
bsatn(U16(x: u16)) = to_little_endian_bytes(x)
bsatn(U32(x: u32)) = to_little_endian_bytes(x)
bsatn(U64(x: u64)) = to_little_endian_bytes(x)
bsatn(U128(x: u128)) = to_little_endian_bytes(x)
bsatn(U256(x: u256)) = to_little_endian_bytes(x)
bsatn(I8(x: i8)) = to_little_endian_bytes(x)
bsatn(I16(x: i16)) = to_little_endian_bytes(x)
bsatn(I32(x: i32)) = to_little_endian_bytes(x)
bsatn(I64(x: i64)) = to_little_endian_bytes(x)
bsatn(I128(x: i128)) = to_little_endian_bytes(x)
bsatn(I256(x: i256)) = to_little_endian_bytes(x)
bsatn(F32(x: f32)) = bsatn(f32_to_raw_bits(x)) // lossless conversion
bsatn(F64(x: f64)) = bsatn(f64_to_raw_bits(x)) // lossless conversion
bsatn(String(s)) = bsatn(len(s) as u32) ++ bsatn(bytes(s))
```

Where

- `f32_to_raw_bits(x)` extracts the raw bits of `x: f32` to `u32`
- `f64_to_raw_bits(x)` extracts the raw bits of `x: f64` to `u64`

## Types

All SATS types are BSATN-encoded by converting them to an `AlgebraicValue`,
then BSATN-encoding that meta-value.

See [the SATN JSON Format](/sats-json)
for more details of the conversion to meta values.
Note that these meta values are converted to BSATN and _not JSON_.
