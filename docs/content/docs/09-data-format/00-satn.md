---
title: SATN JSON Format
navTitle: SATN
---

The Spacetime Algebraic Type Notation JSON format defines how Spacetime `AlgebraicType`s and `AlgebraicValue`s are encoded as JSON. Algebraic types and values are JSON-encoded for transport via the [HTTP Databases API](/docs/http/database) and the [WebSocket text protocol](/docs/ws/overview#protocols-text-protocol).

## Values

### At a glance

| Type             | Description                                                            |
| ---------------- | ---------------------------------------------------------------------- |
| `AlgebraicValue` | A value whose type may be any [`AlgebraicType`](#types-algebraictype). |
| `SumValue`       | A value whose type is a [`SumType`](#types-sumtype).                   |
| `ProductValue`   | A value whose type is a [`ProductType`](#types-producttype).           |
| `BuiltinValue`   | A value whose type is a [`BuiltinType`](#types-builtintype).           |
|                  |                                                                        |

### `AlgebraicValue`

```json
SumValue | ProductValue | BuiltinValue
```

### `SumValue`

An instance of a [`SumType`](#types-sumtype). `SumValue`s are encoded as a JSON object with a single key, a non-negative integer tag which identifies the variant. The value associated with this key is the variant data. Variants which hold no data will have an empty array as their value.

The tag is an index into the [`SumType.variants`](#types-sumtype) array of the value's [`SumType`](#types-sumtype).

```json
{
    "<tag>": AlgebraicValue
}
```

### `ProductValue`

An instance of a [`ProductType`](#types-producttype). `ProductValue`s are encoded as JSON arrays. Each element of the `ProductValue` array is of the type of the corresponding index in the [`ProductType.elements`](#types-producttype) array of the value's [`ProductType`](#types-producttype).

```json
array<AlgebraicValue>
```

### `BuiltinValue`

An instance of a [`BuiltinType`](#types-builtintype). `BuiltinValue`s are encoded as JSON values of corresponding types.

```json
boolean | number | string | array<AlgebraicValue> | map<AlgebraicValue, AlgebraicValue>
```

| [`BuiltinType`](#types-builtintype) | JSON type                             |
| ----------------------------------- | ------------------------------------- |
| `Bool`                              | `boolean`                             |
| Integer types                       | `number`                              |
| Float types                         | `number`                              |
| `String`                            | `string`                              |
| Array types                         | `array<AlgebraicValue>`               |
| Map types                           | `map<AlgebraicValue, AlgebraicValue>` |

All SATS integer types are encoded as JSON `number`s, so values of 64-bit and 128-bit integer types may lose precision when encoding values larger than 2⁵².

## Types

All SATS types are JSON-encoded by converting them to an `AlgebraicValue`, then JSON-encoding that meta-value.

### At a glance

| Type                                          | Description                                                                          |
| --------------------------------------------- | ------------------------------------------------------------------------------------ |
| [`AlgebraicType`](#types-algebraictype)       | Any SATS type.                                                                       |
| [`SumType`](#types-sumtype)                   | Sum types, i.e. tagged unions.                                                       |
| [`ProductType`](#types-producttype)           | Product types, i.e. structures.                                                      |
| [`BuiltinType`](#types-builtintype)           | Built-in and primitive types, including booleans, numbers, strings, arrays and maps. |
| [`AlgebraicTypeRef`](#types-algebraictyperef) | An indirect reference to a type, used to implement recursive types.                  |

### `AlgebraicType`

`AlgebraicType` is the most general meta-type in the Spacetime Algebraic Type System. Any SATS type can be represented as an `AlgebraicType`. `AlgebraicType` is encoded as a tagged union, with variants for [`SumType`](#types-sumtype), [`ProductType`](#types-producttype), [`BuiltinType`](#types-builtintype) and [`AlgebraicTypeRef`](#types-algebraictyperef).

```json
{ "Sum": SumType }
| { "Product": ProductType }
| { "Builtin": BuiltinType }
| { "Ref": AlgebraicTypeRef }
```

### `SumType`

The meta-type `SumType` represents sum types, also called tagged unions or Rust `enum`s. A sum type has some number of variants, each of which has an `AlgebraicType` of variant data, and an optional string discriminant. For each instance, exactly one variant will be active. The instance will contain only that variant's data.

A `SumType` with zero variants is called an empty type or never type because it is impossible to construct an instance.

Instances of `SumType`s are [`SumValue`s](#values-sumvalue), and store a tag which identifies the active variant.

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

Instances of `ProductType`s are [`ProductValue`s](#values-productvalue), and store an array of field data.

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

`AlgebraicTypeRef`s are JSON-encoded as non-negative integers. These are indices into a typespace, like the one returned by the [`/database/schema/:name_or_address GET` HTTP endpoint](/docs/http/database#database-schema-name-or-address-get).
