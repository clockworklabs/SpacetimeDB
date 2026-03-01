package main

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// SimpleEnum mirrors the Rust SimpleEnum enum.
// In BSATN it is a sum type with unit variants, encoded as u8 tag.
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
// NOTE: This is represented as an interface in Go since the Go SDK
// uses reflection-based serialization. Each variant is a concrete type.
// The tag order must match the Rust definition exactly.
type EnumWithPayload interface {
	enumWithPayloadTag() uint8
}

type EnumWithPayloadU8 struct{ Value uint8 }
type EnumWithPayloadU16 struct{ Value uint16 }
type EnumWithPayloadU32 struct{ Value uint32 }
type EnumWithPayloadU64 struct{ Value uint64 }
type EnumWithPayloadU128 struct{ Value types.Uint128 }
type EnumWithPayloadU256 struct{ Value types.Uint256 }
type EnumWithPayloadI8 struct{ Value int8 }
type EnumWithPayloadI16 struct{ Value int16 }
type EnumWithPayloadI32 struct{ Value int32 }
type EnumWithPayloadI64 struct{ Value int64 }
type EnumWithPayloadI128 struct{ Value types.Int128 }
type EnumWithPayloadI256 struct{ Value types.Int256 }
type EnumWithPayloadBool struct{ Value bool }
type EnumWithPayloadF32 struct{ Value float32 }
type EnumWithPayloadF64 struct{ Value float64 }
type EnumWithPayloadStr struct{ Value string }
type EnumWithPayloadIdentity struct{ Value types.Identity }
type EnumWithPayloadConnectionId struct{ Value types.ConnectionId }
type EnumWithPayloadTimestamp struct{ Value types.Timestamp }
type EnumWithPayloadUuid struct{ Value types.Uuid }
type EnumWithPayloadBytes struct{ Value []uint8 }
type EnumWithPayloadInts struct{ Value []int32 }
type EnumWithPayloadStrings struct{ Value []string }
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
// Ok variant (tag 0) holds int32, Err variant (tag 1) holds string.
type ResultI32StringValue interface {
	resultI32StringValueTag() uint8
}

type ResultI32StringOk struct{ Value int32 }
type ResultI32StringErr struct{ Value string }

func (ResultI32StringOk) resultI32StringValueTag() uint8  { return 0 }
func (ResultI32StringErr) resultI32StringValueTag() uint8 { return 1 }

// ResultStringI32Value is a Result<String, i32> sum type.
// Ok variant (tag 0) holds string, Err variant (tag 1) holds int32.
type ResultStringI32Value interface {
	resultStringI32ValueTag() uint8
}

type ResultStringI32Ok struct{ Value string }
type ResultStringI32Err struct{ Value int32 }

func (ResultStringI32Ok) resultStringI32ValueTag() uint8  { return 0 }
func (ResultStringI32Err) resultStringI32ValueTag() uint8 { return 1 }

// ResultIdentityStringValue is a Result<Identity, String> sum type.
// Ok variant (tag 0) holds types.Identity, Err variant (tag 1) holds string.
type ResultIdentityStringValue interface {
	resultIdentityStringValueTag() uint8
}

type ResultIdentityStringOk struct{ Value types.Identity }
type ResultIdentityStringErr struct{ Value string }

func (ResultIdentityStringOk) resultIdentityStringValueTag() uint8  { return 0 }
func (ResultIdentityStringErr) resultIdentityStringValueTag() uint8 { return 1 }

// ResultSimpleEnumI32Value is a Result<SimpleEnum, i32> sum type.
// Ok variant (tag 0) holds SimpleEnum, Err variant (tag 1) holds int32.
type ResultSimpleEnumI32Value interface {
	resultSimpleEnumI32ValueTag() uint8
}

type ResultSimpleEnumI32Ok struct{ Value SimpleEnum }
type ResultSimpleEnumI32Err struct{ Value int32 }

func (ResultSimpleEnumI32Ok) resultSimpleEnumI32ValueTag() uint8  { return 0 }
func (ResultSimpleEnumI32Err) resultSimpleEnumI32ValueTag() uint8 { return 1 }

// ResultEveryPrimitiveStructStringValue is a Result<EveryPrimitiveStruct, String> sum type.
// Ok variant (tag 0) holds EveryPrimitiveStruct, Err variant (tag 1) holds string.
type ResultEveryPrimitiveStructStringValue interface {
	resultEveryPrimitiveStructStringValueTag() uint8
}

type ResultEveryPrimitiveStructStringOk struct{ Value EveryPrimitiveStruct }
type ResultEveryPrimitiveStructStringErr struct{ Value string }

func (ResultEveryPrimitiveStructStringOk) resultEveryPrimitiveStructStringValueTag() uint8 {
	return 0
}
func (ResultEveryPrimitiveStructStringErr) resultEveryPrimitiveStructStringValueTag() uint8 {
	return 1
}

// ResultVecI32StringValue is a Result<Vec<i32>, String> sum type.
// Ok variant (tag 0) holds []int32, Err variant (tag 1) holds string.
type ResultVecI32StringValue interface {
	resultVecI32StringValueTag() uint8
}

type ResultVecI32StringOk struct{ Value []int32 }
type ResultVecI32StringErr struct{ Value string }

func (ResultVecI32StringOk) resultVecI32StringValueTag() uint8  { return 0 }
func (ResultVecI32StringErr) resultVecI32StringValueTag() uint8 { return 1 }
