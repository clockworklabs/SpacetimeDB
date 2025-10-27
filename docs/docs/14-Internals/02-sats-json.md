---
slug: /sats-json
---

# SATS-JSON Data Format

The Spacetime Algebraic Type System JSON format defines how Spacetime `AlgebraicType`s and `AlgebraicValue`s are encoded as JSON. Algebraic types and values are JSON-encoded for transport via the [HTTP Databases API](/http/database) and the WebSocket text protocol. Note that SATS-JSON is not self-describing, and so a SATS value represented in JSON requires knowing the value's schema to meaningfully understand it - for example, it's not possible to tell whether a JSON object with a single field is a `ProductValue` with one element or a `SumValue`.

## Values

### At a glance

| Type             | Description                                                      |
| ---------------- | ---------------------------------------------------------------- |
| `AlgebraicValue` | A value whose type may be any [`AlgebraicType`](#algebraictype). |
| `SumValue`       | A value whose type is a [`SumType`](#sumtype).                   |
| `ProductValue`   | A value whose type is a [`ProductType`](#producttype).           |
| `BuiltinValue`   | A value whose type is a [`BuiltinType`](#builtintype).           |
|                  |                                                                  |

### `AlgebraicValue`

```json
SumValue | ProductValue | BuiltinValue
```

### `SumValue`

An instance of a [`SumType`](#sumtype). `SumValue`s are encoded as a JSON object with a single key, a non-negative integer tag which identifies the variant. The value associated with this key is the variant data. Variants which hold no data will have an empty array as their value.

The tag is an index into the [`SumType.variants`](#sumtype) array of the value's [`SumType`](#sumtype).

```json
{
    "<tag>": AlgebraicValue
}
```

The tag may also be the name of one of the variants.

### `ProductValue`

An instance of a [`ProductType`](#producttype). `ProductValue`s are encoded as JSON arrays. Each element of the `ProductValue` array is of the type of the corresponding index in the [`ProductType.elements`](#producttype) array of the value's [`ProductType`](#producttype).

```json
array<AlgebraicValue>
```

`ProductValue`s may also be encoded as a JSON object with the keys as the field
names of the `ProductValue` and the values as the corresponding
`AlgebraicValue`s.

### `BuiltinValue`

An instance of a [`BuiltinType`](#builtintype). `BuiltinValue`s are encoded as JSON values of corresponding types.

```json
boolean | number | string | array<AlgebraicValue> | map<AlgebraicValue, AlgebraicValue>
```

| [`BuiltinType`](#builtintype) | JSON type                             |
| ----------------------------- | ------------------------------------- |
| `Bool`                        | `boolean`                             |
| Integer types                 | `number`                              |
| Float types                   | `number`                              |
| `String`                      | `string`                              |
| Array types                   | `array<AlgebraicValue>`               |
| Map types                     | `map<AlgebraicValue, AlgebraicValue>` |

All SATS integer types are encoded as JSON `number`s, so values of 64-bit and 128-bit integer types may lose precision when encoding values larger than 2⁵².

## Types

All SATS types are JSON-encoded by converting them to an `AlgebraicValue`, then JSON-encoding that meta-value.

### At a glance

| Type                                    | Description                                                                          |
| --------------------------------------- | ------------------------------------------------------------------------------------ |
| [`AlgebraicType`](#algebraictype)       | Any SATS type.                                                                       |
| [`SumType`](#sumtype)                   | Sum types, i.e. tagged unions.                                                       |
| [`ProductType`](#producttype)           | Product types, i.e. structures.                                                      |
| [`BuiltinType`](#builtintype)           | Built-in and primitive types, including booleans, numbers, strings, arrays and maps. |
| [`AlgebraicTypeRef`](#algebraictyperef) | An indirect reference to a type, used to implement recursive types.                  |

#### `AlgebraicType`

`AlgebraicType` is the most general meta-type in the Spacetime Algebraic Type System. Any SATS type can be represented as an `AlgebraicType`. `AlgebraicType` is encoded as a tagged union, with variants for [`SumType`](#sumtype), [`ProductType`](#producttype), [`BuiltinType`](#builtintype) and [`AlgebraicTypeRef`](#algebraictyperef).

```json
{ "Sum": SumType }
| { "Product": ProductType }
| { "Builtin": BuiltinType }
| { "Ref": AlgebraicTypeRef }
```

#### `SumType`

The meta-type `SumType` represents sum types, also called tagged unions or Rust `enum`s. A sum type has some number of variants, each of which has an `AlgebraicType` of variant data, and an optional string discriminant. For each instance, exactly one variant will be active. The instance will contain only that variant's data.

A `SumType` with zero variants is called an empty type or never type because it is impossible to construct an instance.

Instances of `SumType`s are [`SumValue`s](#sumvalue), and store a tag which identifies the active variant.

```json
// SumType:
{
    "variants": array<SumTypeVariant>,
}

// SumTypeVariant:
{
    "algebraic_type": AlgebraicType,
    "name": { "some": string } | { "none": [] }
}
```

### `ProductType`

The meta-type `ProductType` represents product types, also called structs or tuples. A product type has some number of fields, each of which has an `AlgebraicType` of field data, and an optional string field name. Each instance will contain data for all of the product type's fields.

A `ProductType` with zero fields is called a unit type because it has a single instance, the unit, which is empty.

Instances of `ProductType`s are [`ProductValue`s](#productvalue), and store an array of field data.

```json
// ProductType:
{
    "elements": array<ProductTypeElement>,
}

// ProductTypeElement:
{
    "algebraic_type": AlgebraicType,
    "name": { "some": string } | { "none": [] }
}
```

### `BuiltinType`

The meta-type `BuiltinType` represents SATS primitive types: booleans, integers, floating-point numbers, strings, arrays and maps. `BuiltinType` is encoded as a tagged union, with a variant for each SATS primitive type.

SATS integer types are identified by their signedness and width in bits. SATS supports the same set of integer types as Rust, i.e. 8, 16, 32, 64 and 128-bit signed and unsigned integers.

SATS floating-point number types are identified by their width in bits. SATS supports 32 and 64-bit floats, which correspond to [IEEE 754](https://en.wikipedia.org/wiki/IEEE_754) single- and double-precision binary floats, respectively.

SATS array and map types are homogeneous, meaning that each array has a single element type to which all its elements must conform, and each map has a key type and a value type to which all of its keys and values must conform.

```json
{ "Bool": [] }
| { "I8": [] }
| { "U8": [] }
| { "I16": [] }
| { "U16": [] }
| { "I32": [] }
| { "U32": [] }
| { "I64": [] }
| { "U64": [] }
| { "I128": [] }
| { "U128": [] }
| { "F32": [] }
| { "F64": [] }
| { "String": [] }
| { "Array": AlgebraicType }
| { "Map": {
      "key_ty": AlgebraicType,
      "ty": AlgebraicType,
  } }
```

### `AlgebraicTypeRef`

`AlgebraicTypeRef`s are JSON-encoded as non-negative integers. These are indices into a typespace, like the one returned by the [`GET /v1/database/:name_or_identity/schema` HTTP endpoint](/http/database#get-v1databasename_or_identityschema).
