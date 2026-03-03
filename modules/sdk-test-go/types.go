package main

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// SimpleEnum mirrors the Rust SimpleEnum enum.
// In BSATN it is a sum type with unit variants, encoded as u8 tag.
//
//stdb:enum variants=Zero,One,Two
type SimpleEnum uint8

const (
	SimpleEnumZero SimpleEnum = 0
	SimpleEnumOne  SimpleEnum = 1
	SimpleEnumTwo  SimpleEnum = 2
)

// UnitStruct mirrors the Rust UnitStruct (empty struct).
type UnitStruct struct{}

// ByteStruct mirrors the Rust ByteStruct.
type ByteStruct struct {
	B uint8
}

// EveryPrimitiveStruct mirrors the Rust EveryPrimitiveStruct.
type EveryPrimitiveStruct struct {
	A uint8
	B uint16
	C uint32
	D uint64
	E types.Uint128
	F types.Uint256
	G int8
	H int16
	I int32
	J int64
	K types.Int128
	L types.Int256
	M bool
	N float32
	O float64
	P string
	Q types.Identity
	R types.ConnectionId
	S types.Timestamp
	T types.TimeDuration
	U types.Uuid
}

// EveryVecStruct mirrors the Rust EveryVecStruct.
type EveryVecStruct struct {
	A []uint8
	B []uint16
	C []uint32
	D []uint64
	E []types.Uint128
	F []types.Uint256
	G []int8
	H []int16
	I []int32
	J []int64
	K []types.Int128
	L []types.Int256
	M []bool
	N []float32
	O []float64
	P []string
	Q []types.Identity
	R []types.ConnectionId
	S []types.Timestamp
	T []types.TimeDuration
	U []types.Uuid
}

// EnumWithPayload is a sum type with many variant types.
//
//stdb:sumtype
type EnumWithPayload interface {
	enumWithPayloadTag() uint8
}

//stdb:variant of=EnumWithPayload name=U8
type EnumWithPayloadU8 struct{ Value uint8 }

//stdb:variant of=EnumWithPayload name=U16
type EnumWithPayloadU16 struct{ Value uint16 }

//stdb:variant of=EnumWithPayload name=U32
type EnumWithPayloadU32 struct{ Value uint32 }

//stdb:variant of=EnumWithPayload name=U64
type EnumWithPayloadU64 struct{ Value uint64 }

//stdb:variant of=EnumWithPayload name=U128
type EnumWithPayloadU128 struct{ Value types.Uint128 }

//stdb:variant of=EnumWithPayload name=U256
type EnumWithPayloadU256 struct{ Value types.Uint256 }

//stdb:variant of=EnumWithPayload name=I8
type EnumWithPayloadI8 struct{ Value int8 }

//stdb:variant of=EnumWithPayload name=I16
type EnumWithPayloadI16 struct{ Value int16 }

//stdb:variant of=EnumWithPayload name=I32
type EnumWithPayloadI32 struct{ Value int32 }

//stdb:variant of=EnumWithPayload name=I64
type EnumWithPayloadI64 struct{ Value int64 }

//stdb:variant of=EnumWithPayload name=I128
type EnumWithPayloadI128 struct{ Value types.Int128 }

//stdb:variant of=EnumWithPayload name=I256
type EnumWithPayloadI256 struct{ Value types.Int256 }

//stdb:variant of=EnumWithPayload name=Bool
type EnumWithPayloadBool struct{ Value bool }

//stdb:variant of=EnumWithPayload name=F32
type EnumWithPayloadF32 struct{ Value float32 }

//stdb:variant of=EnumWithPayload name=F64
type EnumWithPayloadF64 struct{ Value float64 }

//stdb:variant of=EnumWithPayload name=Str
type EnumWithPayloadStr struct{ Value string }

//stdb:variant of=EnumWithPayload name=Identity
type EnumWithPayloadIdentity struct{ Value types.Identity }

//stdb:variant of=EnumWithPayload name=ConnectionId
type EnumWithPayloadConnectionId struct{ Value types.ConnectionId }

//stdb:variant of=EnumWithPayload name=Timestamp
type EnumWithPayloadTimestamp struct{ Value types.Timestamp }

//stdb:variant of=EnumWithPayload name=Uuid
type EnumWithPayloadUuid struct{ Value types.Uuid }

//stdb:variant of=EnumWithPayload name=Bytes
type EnumWithPayloadBytes struct{ Value []uint8 }

//stdb:variant of=EnumWithPayload name=Ints
type EnumWithPayloadInts struct{ Value []int32 }

//stdb:variant of=EnumWithPayload name=Strings
type EnumWithPayloadStrings struct{ Value []string }

//stdb:variant of=EnumWithPayload name=SimpleEnums
type EnumWithPayloadSimpleEnums struct{ Value []SimpleEnum }

func (EnumWithPayloadU8) enumWithPayloadTag() uint8           { return 0 }
func (EnumWithPayloadU16) enumWithPayloadTag() uint8          { return 1 }
func (EnumWithPayloadU32) enumWithPayloadTag() uint8          { return 2 }
func (EnumWithPayloadU64) enumWithPayloadTag() uint8          { return 3 }
func (EnumWithPayloadU128) enumWithPayloadTag() uint8         { return 4 }
func (EnumWithPayloadU256) enumWithPayloadTag() uint8         { return 5 }
func (EnumWithPayloadI8) enumWithPayloadTag() uint8           { return 6 }
func (EnumWithPayloadI16) enumWithPayloadTag() uint8          { return 7 }
func (EnumWithPayloadI32) enumWithPayloadTag() uint8          { return 8 }
func (EnumWithPayloadI64) enumWithPayloadTag() uint8          { return 9 }
func (EnumWithPayloadI128) enumWithPayloadTag() uint8         { return 10 }
func (EnumWithPayloadI256) enumWithPayloadTag() uint8         { return 11 }
func (EnumWithPayloadBool) enumWithPayloadTag() uint8         { return 12 }
func (EnumWithPayloadF32) enumWithPayloadTag() uint8          { return 13 }
func (EnumWithPayloadF64) enumWithPayloadTag() uint8          { return 14 }
func (EnumWithPayloadStr) enumWithPayloadTag() uint8          { return 15 }
func (EnumWithPayloadIdentity) enumWithPayloadTag() uint8     { return 16 }
func (EnumWithPayloadConnectionId) enumWithPayloadTag() uint8 { return 17 }
func (EnumWithPayloadTimestamp) enumWithPayloadTag() uint8    { return 18 }
func (EnumWithPayloadUuid) enumWithPayloadTag() uint8         { return 19 }
func (EnumWithPayloadBytes) enumWithPayloadTag() uint8        { return 20 }
func (EnumWithPayloadInts) enumWithPayloadTag() uint8         { return 21 }
func (EnumWithPayloadStrings) enumWithPayloadTag() uint8      { return 22 }
func (EnumWithPayloadSimpleEnums) enumWithPayloadTag() uint8  { return 23 }

// ResultI32StringValue is a Result<i32, String> sum type.
//
//stdb:sumtype
type ResultI32StringValue interface {
	resultI32StringValueTag() uint8
}

//stdb:variant of=ResultI32StringValue name=ok
type ResultI32StringOk struct{ Value int32 }

//stdb:variant of=ResultI32StringValue name=err
type ResultI32StringErr struct{ Value string }

func (ResultI32StringOk) resultI32StringValueTag() uint8  { return 0 }
func (ResultI32StringErr) resultI32StringValueTag() uint8 { return 1 }

// ResultStringI32Value is a Result<String, i32> sum type.
//
//stdb:sumtype
type ResultStringI32Value interface {
	resultStringI32ValueTag() uint8
}

//stdb:variant of=ResultStringI32Value name=ok
type ResultStringI32Ok struct{ Value string }

//stdb:variant of=ResultStringI32Value name=err
type ResultStringI32Err struct{ Value int32 }

func (ResultStringI32Ok) resultStringI32ValueTag() uint8  { return 0 }
func (ResultStringI32Err) resultStringI32ValueTag() uint8 { return 1 }

// ResultIdentityStringValue is a Result<Identity, String> sum type.
//
//stdb:sumtype
type ResultIdentityStringValue interface {
	resultIdentityStringValueTag() uint8
}

//stdb:variant of=ResultIdentityStringValue name=ok
type ResultIdentityStringOk struct{ Value types.Identity }

//stdb:variant of=ResultIdentityStringValue name=err
type ResultIdentityStringErr struct{ Value string }

func (ResultIdentityStringOk) resultIdentityStringValueTag() uint8  { return 0 }
func (ResultIdentityStringErr) resultIdentityStringValueTag() uint8 { return 1 }

// ResultSimpleEnumI32Value is a Result<SimpleEnum, i32> sum type.
//
//stdb:sumtype
type ResultSimpleEnumI32Value interface {
	resultSimpleEnumI32ValueTag() uint8
}

//stdb:variant of=ResultSimpleEnumI32Value name=ok
type ResultSimpleEnumI32Ok struct{ Value SimpleEnum }

//stdb:variant of=ResultSimpleEnumI32Value name=err
type ResultSimpleEnumI32Err struct{ Value int32 }

func (ResultSimpleEnumI32Ok) resultSimpleEnumI32ValueTag() uint8  { return 0 }
func (ResultSimpleEnumI32Err) resultSimpleEnumI32ValueTag() uint8 { return 1 }

// ResultEveryPrimitiveStructStringValue is a Result<EveryPrimitiveStruct, String> sum type.
//
//stdb:sumtype
type ResultEveryPrimitiveStructStringValue interface {
	resultEveryPrimitiveStructStringValueTag() uint8
}

//stdb:variant of=ResultEveryPrimitiveStructStringValue name=ok
type ResultEveryPrimitiveStructStringOk struct{ Value EveryPrimitiveStruct }

//stdb:variant of=ResultEveryPrimitiveStructStringValue name=err
type ResultEveryPrimitiveStructStringErr struct{ Value string }

func (ResultEveryPrimitiveStructStringOk) resultEveryPrimitiveStructStringValueTag() uint8 {
	return 0
}
func (ResultEveryPrimitiveStructStringErr) resultEveryPrimitiveStructStringValueTag() uint8 {
	return 1
}

// ResultVecI32StringValue is a Result<Vec<i32>, String> sum type.
//
//stdb:sumtype
type ResultVecI32StringValue interface {
	resultVecI32StringValueTag() uint8
}

//stdb:variant of=ResultVecI32StringValue name=ok
type ResultVecI32StringOk struct{ Value []int32 }

//stdb:variant of=ResultVecI32StringValue name=err
type ResultVecI32StringErr struct{ Value string }

func (ResultVecI32StringOk) resultVecI32StringValueTag() uint8  { return 0 }
func (ResultVecI32StringErr) resultVecI32StringValueTag() uint8 { return 1 }
