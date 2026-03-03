package main

import "github.com/clockworklabs/SpacetimeDB/sdks/go/types"

// ---------------------------------------------------------------------------
// One* tables -- each holds a single value of a given type.
// ---------------------------------------------------------------------------

//stdb:table name=one_u_8 access=public
//stdb:rls SELECT * FROM one_u_8
type OneU8 struct {
	N uint8
}

//stdb:table name=one_u_16 access=public
type OneU16 struct {
	N uint16
}

//stdb:table name=one_u_32 access=public
type OneU32 struct {
	N uint32
}

//stdb:table name=one_u_64 access=public
type OneU64 struct {
	N uint64
}

//stdb:table name=one_u_128 access=public
type OneU128 struct {
	N types.Uint128
}

//stdb:table name=one_u_256 access=public
type OneU256 struct {
	N types.Uint256
}

//stdb:table name=one_i_8 access=public
type OneI8 struct {
	N int8
}

//stdb:table name=one_i_16 access=public
type OneI16 struct {
	N int16
}

//stdb:table name=one_i_32 access=public
type OneI32 struct {
	N int32
}

//stdb:table name=one_i_64 access=public
type OneI64 struct {
	N int64
}

//stdb:table name=one_i_128 access=public
type OneI128 struct {
	N types.Int128
}

//stdb:table name=one_i_256 access=public
type OneI256 struct {
	N types.Int256
}

//stdb:table name=one_bool access=public
type OneBool struct {
	B bool
}

//stdb:table name=one_f_32 access=public
type OneF32 struct {
	F float32
}

//stdb:table name=one_f_64 access=public
type OneF64 struct {
	F float64
}

//stdb:table name=one_string access=public
type OneString struct {
	S string
}

//stdb:table name=one_identity access=public
type OneIdentity struct {
	I types.Identity
}

//stdb:table name=one_connection_id access=public
type OneConnectionId struct {
	A types.ConnectionId
}

//stdb:table name=one_uuid access=public
type OneUuid struct {
	U types.Uuid
}

//stdb:table name=one_timestamp access=public
type OneTimestamp struct {
	T types.Timestamp
}

//stdb:table name=one_simple_enum access=public
type OneSimpleEnum struct {
	E SimpleEnum
}

//stdb:table name=one_enum_with_payload access=public
type OneEnumWithPayload struct {
	E EnumWithPayload
}

//stdb:table name=one_unit_struct access=public
type OneUnitStruct struct {
	S UnitStruct
}

//stdb:table name=one_byte_struct access=public
type OneByteStruct struct {
	S ByteStruct
}

//stdb:table name=one_every_primitive_struct access=public
type OneEveryPrimitiveStruct struct {
	S EveryPrimitiveStruct
}

//stdb:table name=one_every_vec_struct access=public
type OneEveryVecStruct struct {
	S EveryVecStruct
}

// ---------------------------------------------------------------------------
// Vec* tables -- each holds a slice of a given type.
// ---------------------------------------------------------------------------

//stdb:table name=vec_u_8 access=public
type VecU8 struct {
	N []uint8
}

//stdb:table name=vec_u_16 access=public
type VecU16 struct {
	N []uint16
}

//stdb:table name=vec_u_32 access=public
type VecU32 struct {
	N []uint32
}

//stdb:table name=vec_u_64 access=public
type VecU64 struct {
	N []uint64
}

//stdb:table name=vec_u_128 access=public
type VecU128 struct {
	N []types.Uint128
}

//stdb:table name=vec_u_256 access=public
type VecU256 struct {
	N []types.Uint256
}

//stdb:table name=vec_i_8 access=public
type VecI8 struct {
	N []int8
}

//stdb:table name=vec_i_16 access=public
type VecI16 struct {
	N []int16
}

//stdb:table name=vec_i_32 access=public
type VecI32 struct {
	N []int32
}

//stdb:table name=vec_i_64 access=public
type VecI64 struct {
	N []int64
}

//stdb:table name=vec_i_128 access=public
type VecI128 struct {
	N []types.Int128
}

//stdb:table name=vec_i_256 access=public
type VecI256 struct {
	N []types.Int256
}

//stdb:table name=vec_bool access=public
type VecBool struct {
	B []bool
}

//stdb:table name=vec_f_32 access=public
type VecF32 struct {
	F []float32
}

//stdb:table name=vec_f_64 access=public
type VecF64 struct {
	F []float64
}

//stdb:table name=vec_string access=public
type VecString struct {
	S []string
}

//stdb:table name=vec_identity access=public
type VecIdentity struct {
	I []types.Identity
}

//stdb:table name=vec_connection_id access=public
type VecConnectionId struct {
	A []types.ConnectionId
}

//stdb:table name=vec_uuid access=public
type VecUuid struct {
	U []types.Uuid
}

//stdb:table name=vec_timestamp access=public
type VecTimestamp struct {
	T []types.Timestamp
}

//stdb:table name=vec_simple_enum access=public
type VecSimpleEnum struct {
	E []SimpleEnum
}

//stdb:table name=vec_enum_with_payload access=public
type VecEnumWithPayload struct {
	E []EnumWithPayload
}

//stdb:table name=vec_unit_struct access=public
type VecUnitStruct struct {
	S []UnitStruct
}

//stdb:table name=vec_byte_struct access=public
type VecByteStruct struct {
	S []ByteStruct
}

//stdb:table name=vec_every_primitive_struct access=public
type VecEveryPrimitiveStruct struct {
	S []EveryPrimitiveStruct
}

//stdb:table name=vec_every_vec_struct access=public
type VecEveryVecStruct struct {
	S []EveryVecStruct
}

// ---------------------------------------------------------------------------
// Option* tables -- each holds an optional value (pointer = Option<T>).
// ---------------------------------------------------------------------------

//stdb:table name=option_i_32 access=public
type OptionI32 struct {
	N *int32
}

//stdb:table name=option_string access=public
type OptionString struct {
	S *string
}

//stdb:table name=option_identity access=public
type OptionIdentity struct {
	I *types.Identity
}

//stdb:table name=option_uuid access=public
type OptionUuid struct {
	U *types.Uuid
}

//stdb:table name=option_simple_enum access=public
type OptionSimpleEnum struct {
	E *SimpleEnum
}

//stdb:table name=option_every_primitive_struct access=public
type OptionEveryPrimitiveStruct struct {
	S *EveryPrimitiveStruct
}

//stdb:table name=option_vec_option_i_32 access=public
type OptionVecOptionI32 struct {
	V *[]*int32
}

// ---------------------------------------------------------------------------
// Result* tables -- Result is a sum type with Ok/Err variants.
// ---------------------------------------------------------------------------

//stdb:table name=result_i_32_string access=public
type ResultI32String struct {
	R ResultI32StringValue
}

//stdb:table name=result_string_i_32 access=public
type ResultStringI32 struct {
	R ResultStringI32Value
}

//stdb:table name=result_identity_string access=public
type ResultIdentityString struct {
	R ResultIdentityStringValue
}

//stdb:table name=result_simple_enum_i_32 access=public
type ResultSimpleEnumI32 struct {
	R ResultSimpleEnumI32Value
}

//stdb:table name=result_every_primitive_struct_string access=public
type ResultEveryPrimitiveStructString struct {
	R ResultEveryPrimitiveStructStringValue
}

//stdb:table name=result_vec_i_32_string access=public
type ResultVecI32String struct {
	R ResultVecI32StringValue
}

// ---------------------------------------------------------------------------
// Unique* tables -- each has a unique (non-pk) key field and an i32 Data payload.
// ---------------------------------------------------------------------------

//stdb:table name=unique_u_8 access=public
type UniqueU8 struct {
	N    uint8 `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_u_16 access=public
type UniqueU16 struct {
	N    uint16 `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_u_32 access=public
type UniqueU32 struct {
	N    uint32 `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_u_64 access=public
type UniqueU64 struct {
	N    uint64 `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_u_128 access=public
type UniqueU128 struct {
	N    types.Uint128 `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_u_256 access=public
type UniqueU256 struct {
	N    types.Uint256 `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_i_8 access=public
type UniqueI8 struct {
	N    int8 `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_i_16 access=public
type UniqueI16 struct {
	N    int16 `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_i_32 access=public
type UniqueI32 struct {
	N    int32 `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_i_64 access=public
type UniqueI64 struct {
	N    int64 `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_i_128 access=public
type UniqueI128 struct {
	N    types.Int128 `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_i_256 access=public
type UniqueI256 struct {
	N    types.Int256 `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_bool access=public
type UniqueBool struct {
	B    bool `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_string access=public
type UniqueString struct {
	S    string `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_identity access=public
type UniqueIdentity struct {
	I    types.Identity `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_connection_id access=public
type UniqueConnectionId struct {
	A    types.ConnectionId `stdb:"unique"`
	Data int32
}

//stdb:table name=unique_uuid access=public
type UniqueUuid struct {
	U    types.Uuid `stdb:"unique"`
	Data int32
}

// ---------------------------------------------------------------------------
// Pk* tables -- each has a primary key field and an i32 Data payload.
// ---------------------------------------------------------------------------

//stdb:table name=pk_u_8 access=public
type PkU8 struct {
	N    uint8 `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_u_16 access=public
type PkU16 struct {
	N    uint16 `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_u_32 access=public
type PkU32 struct {
	N    uint32 `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_u_32_two access=public
type PkU32Two struct {
	N    uint32 `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_u_64 access=public
type PkU64 struct {
	N    uint64 `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_u_128 access=public
type PkU128 struct {
	N    types.Uint128 `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_u_256 access=public
type PkU256 struct {
	N    types.Uint256 `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_i_8 access=public
type PkI8 struct {
	N    int8 `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_i_16 access=public
type PkI16 struct {
	N    int16 `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_i_32 access=public
type PkI32 struct {
	N    int32 `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_i_64 access=public
type PkI64 struct {
	N    int64 `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_i_128 access=public
type PkI128 struct {
	N    types.Int128 `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_i_256 access=public
type PkI256 struct {
	N    types.Int256 `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_bool access=public
type PkBool struct {
	B    bool `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_string access=public
type PkString struct {
	S    string `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_identity access=public
type PkIdentity struct {
	I    types.Identity `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_connection_id access=public
type PkConnectionId struct {
	A    types.ConnectionId `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_uuid access=public
type PkUuid struct {
	U    types.Uuid `stdb:"primarykey"`
	Data int32
}

//stdb:table name=pk_simple_enum access=public
type PkSimpleEnum struct {
	A    SimpleEnum `stdb:"primarykey"`
	Data int32
}

// ---------------------------------------------------------------------------
// Special tables
// ---------------------------------------------------------------------------

// BTreeU32 has a btree index on the N field.
//
//stdb:table name=btree_u32 access=public
type BTreeU32 struct {
	N    uint32 `stdb:"index=btree"`
	Data int32
}

// Users table with primary key on Identity.
//
//stdb:table name=users access=public
//stdb:rls SELECT * FROM users WHERE identity = :sender
type Users struct {
	Identity types.Identity `stdb:"primarykey"`
	Name     string
}

// IndexedTable has a single-column btree index on PlayerId (private table).
//
//stdb:table name=indexed_table access=private
type IndexedTable struct {
	PlayerId uint32 `stdb:"index=btree"`
}

// IndexedTable2 has two columns (private table).
//
//stdb:table name=indexed_table_2 access=private
type IndexedTable2 struct {
	PlayerId    uint32
	PlayerSnazz float32
}

// IndexedSimpleEnum has a btree index on N.
//
//stdb:table name=indexed_simple_enum access=public
type IndexedSimpleEnum struct {
	N SimpleEnum `stdb:"index=btree"`
}

// LargeTable has many fields of many different types.
//
//stdb:table name=large_table access=public
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
//
//stdb:table name=table_holds_table access=public
type TableHoldsTable struct {
	A OneU8
	B VecU8
}

// ScheduledTable is a scheduled table with auto-incrementing primary key.
//
//stdb:table name=scheduled_table access=public
//stdb:schedule table=scheduled_table function=send_scheduled_message
type ScheduledTable struct {
	ScheduledId uint64         `stdb:"primarykey,autoinc"`
	ScheduledAt types.ScheduleAt
	Text        string
}
