package types

import "github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"

// AlgebraicType describes a type in the SpacetimeDB type system.
// Each variant is serialized as a BSATN sum type with a specific tag.
type AlgebraicType interface {
	bsatn.Serializable
	algebraicTypeTag() uint8
}

// TypeRef is an index into a Typespace.
type TypeRef uint32

// Primitive type constructors.

// AlgTypeBool returns an AlgebraicType for bool (tag 5).
func AlgTypeBool() AlgebraicType { return &algTypePrimitive{tag: 5} }

// AlgTypeI8 returns an AlgebraicType for i8 (tag 6).
func AlgTypeI8() AlgebraicType { return &algTypePrimitive{tag: 6} }

// AlgTypeU8 returns an AlgebraicType for u8 (tag 7).
func AlgTypeU8() AlgebraicType { return &algTypePrimitive{tag: 7} }

// AlgTypeI16 returns an AlgebraicType for i16 (tag 8).
func AlgTypeI16() AlgebraicType { return &algTypePrimitive{tag: 8} }

// AlgTypeU16 returns an AlgebraicType for u16 (tag 9).
func AlgTypeU16() AlgebraicType { return &algTypePrimitive{tag: 9} }

// AlgTypeI32 returns an AlgebraicType for i32 (tag 10).
func AlgTypeI32() AlgebraicType { return &algTypePrimitive{tag: 10} }

// AlgTypeU32 returns an AlgebraicType for u32 (tag 11).
func AlgTypeU32() AlgebraicType { return &algTypePrimitive{tag: 11} }

// AlgTypeI64 returns an AlgebraicType for i64 (tag 12).
func AlgTypeI64() AlgebraicType { return &algTypePrimitive{tag: 12} }

// AlgTypeU64 returns an AlgebraicType for u64 (tag 13).
func AlgTypeU64() AlgebraicType { return &algTypePrimitive{tag: 13} }

// AlgTypeI128 returns an AlgebraicType for i128 (tag 14).
func AlgTypeI128() AlgebraicType { return &algTypePrimitive{tag: 14} }

// AlgTypeU128 returns an AlgebraicType for u128 (tag 15).
func AlgTypeU128() AlgebraicType { return &algTypePrimitive{tag: 15} }

// AlgTypeI256 returns an AlgebraicType for i256 (tag 16).
func AlgTypeI256() AlgebraicType { return &algTypePrimitive{tag: 16} }

// AlgTypeU256 returns an AlgebraicType for u256 (tag 17).
func AlgTypeU256() AlgebraicType { return &algTypePrimitive{tag: 17} }

// AlgTypeF32 returns an AlgebraicType for f32 (tag 18).
func AlgTypeF32() AlgebraicType { return &algTypePrimitive{tag: 18} }

// AlgTypeF64 returns an AlgebraicType for f64 (tag 19).
func AlgTypeF64() AlgebraicType { return &algTypePrimitive{tag: 19} }

// AlgTypeString returns an AlgebraicType for string (tag 4).
func AlgTypeString() AlgebraicType { return &algTypePrimitive{tag: 4} }

// Compound type constructors.

// AlgTypeRef returns an AlgebraicType referencing another type by index (tag 0).
func AlgTypeRef(ref TypeRef) AlgebraicType { return &algTypeRef{ref: ref} }

// AlgTypeArray returns an AlgebraicType for a homogeneous array (tag 3).
func AlgTypeArray(elemType AlgebraicType) AlgebraicType { return &algTypeArray{elem: elemType} }

// AlgTypeMap returns an AlgebraicType for a map from key to value (tag 20).
func AlgTypeMap(keyType, valueType AlgebraicType) AlgebraicType {
	return &algTypeMap{key: keyType, value: valueType}
}

// AlgTypeProduct returns an AlgebraicType wrapping a ProductType (tag 2).
func AlgTypeProduct(pt ProductType) AlgebraicType { return &algTypeProduct{pt: pt} }

// AlgTypeSum returns an AlgebraicType wrapping a SumType (tag 1).
func AlgTypeSum(st SumType) AlgebraicType { return &algTypeSum{st: st} }

// algTypePrimitive represents primitive/scalar types that carry no payload.
type algTypePrimitive struct {
	tag uint8
}

func (a *algTypePrimitive) algebraicTypeTag() uint8 { return a.tag }

func (a *algTypePrimitive) WriteBsatn(w bsatn.Writer) {
	w.PutSumTag(a.tag)
	// Primitive types are unit variants (empty product payload).
}

// algTypeRef is a reference to another type in the Typespace.
type algTypeRef struct {
	ref TypeRef
}

func (a *algTypeRef) algebraicTypeTag() uint8 { return 0 }

func (a *algTypeRef) WriteBsatn(w bsatn.Writer) {
	w.PutSumTag(0)
	w.PutU32(uint32(a.ref))
}

// algTypeArray describes an array of a single element type.
type algTypeArray struct {
	elem AlgebraicType
}

func (a *algTypeArray) algebraicTypeTag() uint8 { return 3 }

func (a *algTypeArray) WriteBsatn(w bsatn.Writer) {
	w.PutSumTag(3)
	a.elem.WriteBsatn(w)
}

// algTypeMap describes a map with key and value types.
type algTypeMap struct {
	key, value AlgebraicType
}

func (a *algTypeMap) algebraicTypeTag() uint8 { return 20 }

func (a *algTypeMap) WriteBsatn(w bsatn.Writer) {
	w.PutSumTag(20)
	// MapType is a product of (key_ty, value_ty).
	a.key.WriteBsatn(w)
	a.value.WriteBsatn(w)
}

// algTypeProduct wraps a ProductType as an AlgebraicType.
type algTypeProduct struct {
	pt ProductType
}

func (a *algTypeProduct) algebraicTypeTag() uint8 { return 2 }

func (a *algTypeProduct) WriteBsatn(w bsatn.Writer) {
	w.PutSumTag(2)
	a.pt.WriteBsatn(w)
}

// algTypeSum wraps a SumType as an AlgebraicType.
type algTypeSum struct {
	st SumType
}

func (a *algTypeSum) algebraicTypeTag() uint8 { return 1 }

func (a *algTypeSum) WriteBsatn(w bsatn.Writer) {
	w.PutSumTag(1)
	a.st.WriteBsatn(w)
}
