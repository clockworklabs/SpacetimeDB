package main

import "github.com/clockworklabs/SpacetimeDB/sdks/go/types"

// ---------------------------------------------------------------------------
// One* tables -- each holds a single value of a given type.
// ---------------------------------------------------------------------------

type OneU8 struct {
	N uint8
}

type OneU16 struct {
	N uint16
}

type OneU32 struct {
	N uint32
}

type OneU64 struct {
	N uint64
}

type OneU128 struct {
	N types.Uint128
}

type OneU256 struct {
	N types.Uint256
}

type OneI8 struct {
	N int8
}

type OneI16 struct {
	N int16
}

type OneI32 struct {
	N int32
}

type OneI64 struct {
	N int64
}

type OneI128 struct {
	N types.Int128
}

type OneI256 struct {
	N types.Int256
}

type OneBool struct {
	B bool
}

type OneF32 struct {
	F float32
}

type OneF64 struct {
	F float64
}

type OneString struct {
	S string
}

type OneIdentity struct {
	I types.Identity
}

type OneConnectionId struct {
	A types.ConnectionId
}

type OneUuid struct {
	U types.Uuid
}

type OneTimestamp struct {
	T types.Timestamp
}

type OneSimpleEnum struct {
	E SimpleEnum
}

type OneEnumWithPayload struct {
	E EnumWithPayload
}

type OneUnitStruct struct {
	S UnitStruct
}

type OneByteStruct struct {
	S ByteStruct
}

type OneEveryPrimitiveStruct struct {
	S EveryPrimitiveStruct
}

type OneEveryVecStruct struct {
	S EveryVecStruct
}

// ---------------------------------------------------------------------------
// Vec* tables -- each holds a slice of a given type.
// ---------------------------------------------------------------------------

type VecU8 struct {
	N []uint8
}

type VecU16 struct {
	N []uint16
}

type VecU32 struct {
	N []uint32
}

type VecU64 struct {
	N []uint64
}

type VecU128 struct {
	N []types.Uint128
}

type VecU256 struct {
	N []types.Uint256
}

type VecI8 struct {
	N []int8
}

type VecI16 struct {
	N []int16
}

type VecI32 struct {
	N []int32
}

type VecI64 struct {
	N []int64
}

type VecI128 struct {
	N []types.Int128
}

type VecI256 struct {
	N []types.Int256
}

type VecBool struct {
	B []bool
}

type VecF32 struct {
	F []float32
}

type VecF64 struct {
	F []float64
}

type VecString struct {
	S []string
}

type VecIdentity struct {
	I []types.Identity
}

type VecConnectionId struct {
	A []types.ConnectionId
}

type VecUuid struct {
	U []types.Uuid
}

type VecTimestamp struct {
	T []types.Timestamp
}

type VecSimpleEnum struct {
	E []SimpleEnum
}

type VecEnumWithPayload struct {
	E []EnumWithPayload
}

type VecUnitStruct struct {
	S []UnitStruct
}

type VecByteStruct struct {
	S []ByteStruct
}

type VecEveryPrimitiveStruct struct {
	S []EveryPrimitiveStruct
}

type VecEveryVecStruct struct {
	S []EveryVecStruct
}

// ---------------------------------------------------------------------------
// Option* tables -- each holds an optional value (pointer = Option<T>).
// ---------------------------------------------------------------------------

type OptionI32 struct {
	N *int32
}

type OptionString struct {
	S *string
}

type OptionIdentity struct {
	I *types.Identity
}

type OptionUuid struct {
	U *types.Uuid
}

type OptionSimpleEnum struct {
	E *SimpleEnum
}

type OptionEveryPrimitiveStruct struct {
	S *EveryPrimitiveStruct
}

type OptionVecOptionI32 struct {
	V *[]*int32
}

// ---------------------------------------------------------------------------
// Result* tables -- Result is a sum type with Ok/Err variants.
// The result value types are defined in types.go.
// ---------------------------------------------------------------------------

type ResultI32String struct {
	R ResultI32StringValue
}

type ResultStringI32 struct {
	R ResultStringI32Value
}

type ResultIdentityString struct {
	R ResultIdentityStringValue
}

type ResultSimpleEnumI32 struct {
	R ResultSimpleEnumI32Value
}

type ResultEveryPrimitiveStructString struct {
	R ResultEveryPrimitiveStructStringValue
}

type ResultVecI32String struct {
	R ResultVecI32StringValue
}

// ---------------------------------------------------------------------------
// Unique* tables -- each has a unique (non-pk) key field and an i32 Data payload.
// ---------------------------------------------------------------------------

type UniqueU8 struct {
	N    uint8 `stdb:"unique"`
	Data int32
}

type UniqueU16 struct {
	N    uint16 `stdb:"unique"`
	Data int32
}

type UniqueU32 struct {
	N    uint32 `stdb:"unique"`
	Data int32
}

type UniqueU64 struct {
	N    uint64 `stdb:"unique"`
	Data int32
}

type UniqueU128 struct {
	N    types.Uint128 `stdb:"unique"`
	Data int32
}

type UniqueU256 struct {
	N    types.Uint256 `stdb:"unique"`
	Data int32
}

type UniqueI8 struct {
	N    int8 `stdb:"unique"`
	Data int32
}

type UniqueI16 struct {
	N    int16 `stdb:"unique"`
	Data int32
}

type UniqueI32 struct {
	N    int32 `stdb:"unique"`
	Data int32
}

type UniqueI64 struct {
	N    int64 `stdb:"unique"`
	Data int32
}

type UniqueI128 struct {
	N    types.Int128 `stdb:"unique"`
	Data int32
}

type UniqueI256 struct {
	N    types.Int256 `stdb:"unique"`
	Data int32
}

type UniqueBool struct {
	B    bool `stdb:"unique"`
	Data int32
}

type UniqueString struct {
	S    string `stdb:"unique"`
	Data int32
}

type UniqueIdentity struct {
	I    types.Identity `stdb:"unique"`
	Data int32
}

type UniqueConnectionId struct {
	A    types.ConnectionId `stdb:"unique"`
	Data int32
}

type UniqueUuid struct {
	U    types.Uuid `stdb:"unique"`
	Data int32
}

// ---------------------------------------------------------------------------
// Pk* tables -- each has a primary key field and an i32 Data payload.
// ---------------------------------------------------------------------------

type PkU8 struct {
	N    uint8 `stdb:"primarykey"`
	Data int32
}

type PkU16 struct {
	N    uint16 `stdb:"primarykey"`
	Data int32
}

type PkU32 struct {
	N    uint32 `stdb:"primarykey"`
	Data int32
}

type PkU32Two struct {
	N    uint32 `stdb:"primarykey"`
	Data int32
}

type PkU64 struct {
	N    uint64 `stdb:"primarykey"`
	Data int32
}

type PkU128 struct {
	N    types.Uint128 `stdb:"primarykey"`
	Data int32
}

type PkU256 struct {
	N    types.Uint256 `stdb:"primarykey"`
	Data int32
}

type PkI8 struct {
	N    int8 `stdb:"primarykey"`
	Data int32
}

type PkI16 struct {
	N    int16 `stdb:"primarykey"`
	Data int32
}

type PkI32 struct {
	N    int32 `stdb:"primarykey"`
	Data int32
}

type PkI64 struct {
	N    int64 `stdb:"primarykey"`
	Data int32
}

type PkI128 struct {
	N    types.Int128 `stdb:"primarykey"`
	Data int32
}

type PkI256 struct {
	N    types.Int256 `stdb:"primarykey"`
	Data int32
}

type PkBool struct {
	B    bool `stdb:"primarykey"`
	Data int32
}

type PkString struct {
	S    string `stdb:"primarykey"`
	Data int32
}

type PkIdentity struct {
	I    types.Identity `stdb:"primarykey"`
	Data int32
}

type PkConnectionId struct {
	A    types.ConnectionId `stdb:"primarykey"`
	Data int32
}

type PkUuid struct {
	U    types.Uuid `stdb:"primarykey"`
	Data int32
}

type PkSimpleEnum struct {
	A    SimpleEnum `stdb:"primarykey"`
	Data int32
}

// ---------------------------------------------------------------------------
// Special tables
// ---------------------------------------------------------------------------

// BTreeU32 has a btree index on the N field.
type BTreeU32 struct {
	N    uint32 `stdb:"index=btree"`
	Data int32
}

// Users table with primary key on Identity.
type Users struct {
	Identity types.Identity `stdb:"primarykey"`
	Name     string
}

// IndexedTable has a single-column btree index on PlayerId (private table).
type IndexedTable struct {
	PlayerId uint32 `stdb:"index=btree"`
}

// IndexedTable2 has two columns (private table).
type IndexedTable2 struct {
	PlayerId    uint32
	PlayerSnazz float32
}

// IndexedSimpleEnum has a btree index on N.
type IndexedSimpleEnum struct {
	N SimpleEnum `stdb:"index=btree"`
}

// LargeTable has many fields of many different types.
type LargeTable struct {
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
	Q SimpleEnum
	R EnumWithPayload
	S UnitStruct
	T ByteStruct
	U EveryPrimitiveStruct
	V EveryVecStruct
}

// TableHoldsTable holds instances of other table structs.
type TableHoldsTable struct {
	A OneU8
	B VecU8
}

// ScheduledTable is a scheduled table with auto-incrementing primary key.
type ScheduledTable struct {
	ScheduledId uint64         `stdb:"primarykey,autoinc"`
	ScheduledAt types.ScheduleAt
	Text        string
}

// Table and sum type registrations are in init.go to ensure correct ordering.
