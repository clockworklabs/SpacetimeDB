# SATN Binary Format (BSATN)

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

| Type             | Description                                                      |
| ---------------- | ---------------------------------------------------------------- |
| `AlgebraicValue` | A value whose type may be any [`AlgebraicType`](#algebraictype). |
| `SumValue`       | A value whose type is a [`SumType`](#sumtype).                   |
| `ProductValue`   | A value whose type is a [`ProductType`](#producttype).           |
| `BuiltinValue`   | A value whose type is a [`BuiltinType`](#builtintype).           |

### `AlgebraicValue`

The BSATN encoding of an `AlgebraicValue` defers to the encoding of each variant:

```fsharp
bsatn(AlgebraicValue) = bsatn(SumValue) | bsatn(ProductValue) | bsatn(BuiltinValue)
```

### `SumValue`

An instance of a [`SumType`](#sumtype).
`SumValue`s are binary-encoded as `bsatn(tag) ++ bsatn(variant_data)`
where `tag: u8` is an index into the [`SumType.variants`](#sumtype)
array of the value's [`SumType`](#sumtype),
and where `variant_data` is the data of the variant.
For variants holding no data, i.e., of some zero sized type,
`bsatn(variant_data) = []`.

### `ProductValue`

An instance of a [`ProductType`](#producttype).
`ProductValue`s are binary encoded as:

```fsharp
bsatn(elems) = bsatn(elem_0) ++ .. ++ bsatn(elem_n)
```

Field names are not encoded.

### `BuiltinValue`

An instance of a [`BuiltinType`](#builtintype).
The BSATN encoding of `BuiltinValue`s defers to the encoding of each variant:

```fsharp
bsatn(BuiltinValue)
    = bsatn(Bool)
    | bsatn(U8) | bsatn(U16) | bsatn(U32) | bsatn(U64) | bsatn(U128)
    | bsatn(I8) | bsatn(I16) | bsatn(I32) | bsatn(I64) | bsatn(I128)
    | bsatn(F32) | bsatn(F64)
    | bsatn(String)
    | bsatn(Array)
    | bsatn(Map)

bsatn(Bool(b)) = bsatn(b as u8)
bsatn(U8(x)) = [x]
bsatn(U16(x: u16)) = to_little_endian_bytes(x)
bsatn(U32(x: u32)) = to_little_endian_bytes(x)
bsatn(U64(x: u64)) = to_little_endian_bytes(x)
bsatn(U128(x: u128)) = to_little_endian_bytes(x)
bsatn(I8(x: i8)) = to_little_endian_bytes(x)
bsatn(I16(x: i16)) = to_little_endian_bytes(x)
bsatn(I32(x: i32)) = to_little_endian_bytes(x)
bsatn(I64(x: i64)) = to_little_endian_bytes(x)
bsatn(I128(x: i128)) = to_little_endian_bytes(x)
bsatn(F32(x: f32)) = bsatn(f32_to_raw_bits(x)) // lossless conversion
bsatn(F64(x: f64)) = bsatn(f64_to_raw_bits(x)) // lossless conversion
bsatn(String(s)) = bsatn(len(s) as u32) ++ bsatn(bytes(s))
bsatn(Array(a)) = bsatn(len(a) as u32)
               ++ bsatn(normalize(a)_0) ++ .. ++ bsatn(normalize(a)_n)
bsatn(Map(map)) = bsatn(len(m) as u32)
               ++ bsatn(key(map_0)) ++ bsatn(value(map_0))
               ..
               ++ bsatn(key(map_n)) ++ bsatn(value(map_n))
```

Where

- `f32_to_raw_bits(x)` is the raw transmute of `x: f32` to `u32`
- `f64_to_raw_bits(x)` is the raw transmute of `x: f64` to `u64`
- `normalize(a)` for `a: ArrayValue` converts `a` to a list of `AlgebraicValue`s
- `key(map_i)` extracts the key of the `i`th entry of `map`
- `value(map_i)` extracts the value of the `i`th entry of `map`

## Types

All SATS types are BSATN-encoded by converting them to an `AlgebraicValue`,
then BSATN-encoding that meta-value.

See [the SATN JSON Format](/docs/satn)
for more details of the conversion to meta values.
Note that these meta values are converted to BSATN and _not JSON_.
