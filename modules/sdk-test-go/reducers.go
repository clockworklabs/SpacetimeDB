package main

import (
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// ---------------------------------------------------------------------------
// One* insert reducers -- each inserts a single-field row.
// ---------------------------------------------------------------------------

//stdb:reducer
func insertOneU8(_ server.ReducerContext, n uint8) {
	OneU8Table.Insert(OneU8{N: n})
}

//stdb:reducer
func insertOneU16(_ server.ReducerContext, n uint16) {
	OneU16Table.Insert(OneU16{N: n})
}

//stdb:reducer
func insertOneU32(_ server.ReducerContext, n uint32) {
	OneU32Table.Insert(OneU32{N: n})
}

//stdb:reducer
func insertOneU64(_ server.ReducerContext, n uint64) {
	OneU64Table.Insert(OneU64{N: n})
}

//stdb:reducer
func insertOneU128(_ server.ReducerContext, n types.Uint128) {
	OneU128Table.Insert(OneU128{N: n})
}

//stdb:reducer
func insertOneU256(_ server.ReducerContext, n types.Uint256) {
	OneU256Table.Insert(OneU256{N: n})
}

//stdb:reducer
func insertOneI8(_ server.ReducerContext, n int8) {
	OneI8Table.Insert(OneI8{N: n})
}

//stdb:reducer
func insertOneI16(_ server.ReducerContext, n int16) {
	OneI16Table.Insert(OneI16{N: n})
}

//stdb:reducer
func insertOneI32(_ server.ReducerContext, n int32) {
	OneI32Table.Insert(OneI32{N: n})
}

//stdb:reducer
func insertOneI64(_ server.ReducerContext, n int64) {
	OneI64Table.Insert(OneI64{N: n})
}

//stdb:reducer
func insertOneI128(_ server.ReducerContext, n types.Int128) {
	OneI128Table.Insert(OneI128{N: n})
}

//stdb:reducer
func insertOneI256(_ server.ReducerContext, n types.Int256) {
	OneI256Table.Insert(OneI256{N: n})
}

//stdb:reducer
func insertOneBool(_ server.ReducerContext, b bool) {
	OneBoolTable.Insert(OneBool{B: b})
}

//stdb:reducer
func insertOneF32(_ server.ReducerContext, f float32) {
	OneF32Table.Insert(OneF32{F: f})
}

//stdb:reducer
func insertOneF64(_ server.ReducerContext, f float64) {
	OneF64Table.Insert(OneF64{F: f})
}

//stdb:reducer
func insertOneString(_ server.ReducerContext, s string) {
	OneStringTable.Insert(OneString{S: s})
}

//stdb:reducer
func insertOneIdentity(_ server.ReducerContext, i types.Identity) {
	OneIdentityTable.Insert(OneIdentity{I: i})
}

//stdb:reducer
func insertOneConnectionId(_ server.ReducerContext, a types.ConnectionId) {
	OneConnectionIdTable.Insert(OneConnectionId{A: a})
}

//stdb:reducer
func insertOneUuid(_ server.ReducerContext, u types.Uuid) {
	OneUuidTable.Insert(OneUuid{U: u})
}

//stdb:reducer
func insertOneTimestamp(_ server.ReducerContext, t types.Timestamp) {
	OneTimestampTable.Insert(OneTimestamp{T: t})
}

//stdb:reducer
func insertOneSimpleEnum(_ server.ReducerContext, e SimpleEnum) {
	OneSimpleEnumTable.Insert(OneSimpleEnum{E: e})
}

//stdb:reducer
func insertOneEnumWithPayload(_ server.ReducerContext, e EnumWithPayload) {
	OneEnumWithPayloadTable.Insert(OneEnumWithPayload{E: e})
}

//stdb:reducer
func insertOneUnitStruct(_ server.ReducerContext, s UnitStruct) {
	OneUnitStructTable.Insert(OneUnitStruct{S: s})
}

//stdb:reducer
func insertOneByteStruct(_ server.ReducerContext, s ByteStruct) {
	OneByteStructTable.Insert(OneByteStruct{S: s})
}

//stdb:reducer
func insertOneEveryPrimitiveStruct(_ server.ReducerContext, s EveryPrimitiveStruct) {
	OneEveryPrimitiveStructTable.Insert(OneEveryPrimitiveStruct{S: s})
}

//stdb:reducer
func insertOneEveryVecStruct(_ server.ReducerContext, s EveryVecStruct) {
	OneEveryVecStructTable.Insert(OneEveryVecStruct{S: s})
}

// ---------------------------------------------------------------------------
// Vec* insert reducers -- each inserts a row containing a slice.
// ---------------------------------------------------------------------------

//stdb:reducer
func insertVecU8(_ server.ReducerContext, n []uint8) {
	VecU8Table.Insert(VecU8{N: n})
}

//stdb:reducer
func insertVecU16(_ server.ReducerContext, n []uint16) {
	VecU16Table.Insert(VecU16{N: n})
}

//stdb:reducer
func insertVecU32(_ server.ReducerContext, n []uint32) {
	VecU32Table.Insert(VecU32{N: n})
}

//stdb:reducer
func insertVecU64(_ server.ReducerContext, n []uint64) {
	VecU64Table.Insert(VecU64{N: n})
}

//stdb:reducer
func insertVecU128(_ server.ReducerContext, n []types.Uint128) {
	VecU128Table.Insert(VecU128{N: n})
}

//stdb:reducer
func insertVecU256(_ server.ReducerContext, n []types.Uint256) {
	VecU256Table.Insert(VecU256{N: n})
}

//stdb:reducer
func insertVecI8(_ server.ReducerContext, n []int8) {
	VecI8Table.Insert(VecI8{N: n})
}

//stdb:reducer
func insertVecI16(_ server.ReducerContext, n []int16) {
	VecI16Table.Insert(VecI16{N: n})
}

//stdb:reducer
func insertVecI32(_ server.ReducerContext, n []int32) {
	VecI32Table.Insert(VecI32{N: n})
}

//stdb:reducer
func insertVecI64(_ server.ReducerContext, n []int64) {
	VecI64Table.Insert(VecI64{N: n})
}

//stdb:reducer
func insertVecI128(_ server.ReducerContext, n []types.Int128) {
	VecI128Table.Insert(VecI128{N: n})
}

//stdb:reducer
func insertVecI256(_ server.ReducerContext, n []types.Int256) {
	VecI256Table.Insert(VecI256{N: n})
}

//stdb:reducer
func insertVecBool(_ server.ReducerContext, b []bool) {
	VecBoolTable.Insert(VecBool{B: b})
}

//stdb:reducer
func insertVecF32(_ server.ReducerContext, f []float32) {
	VecF32Table.Insert(VecF32{F: f})
}

//stdb:reducer
func insertVecF64(_ server.ReducerContext, f []float64) {
	VecF64Table.Insert(VecF64{F: f})
}

//stdb:reducer
func insertVecString(_ server.ReducerContext, s []string) {
	VecStringTable.Insert(VecString{S: s})
}

//stdb:reducer
func insertVecIdentity(_ server.ReducerContext, i []types.Identity) {
	VecIdentityTable.Insert(VecIdentity{I: i})
}

//stdb:reducer
func insertVecConnectionId(_ server.ReducerContext, a []types.ConnectionId) {
	VecConnectionIdTable.Insert(VecConnectionId{A: a})
}

//stdb:reducer
func insertVecUuid(_ server.ReducerContext, u []types.Uuid) {
	VecUuidTable.Insert(VecUuid{U: u})
}

//stdb:reducer
func insertVecTimestamp(_ server.ReducerContext, t []types.Timestamp) {
	VecTimestampTable.Insert(VecTimestamp{T: t})
}

//stdb:reducer
func insertVecSimpleEnum(_ server.ReducerContext, e []SimpleEnum) {
	VecSimpleEnumTable.Insert(VecSimpleEnum{E: e})
}

//stdb:reducer
func insertVecEnumWithPayload(_ server.ReducerContext, e []EnumWithPayload) {
	VecEnumWithPayloadTable.Insert(VecEnumWithPayload{E: e})
}

//stdb:reducer
func insertVecUnitStruct(_ server.ReducerContext, s []UnitStruct) {
	VecUnitStructTable.Insert(VecUnitStruct{S: s})
}

//stdb:reducer
func insertVecByteStruct(_ server.ReducerContext, s []ByteStruct) {
	VecByteStructTable.Insert(VecByteStruct{S: s})
}

//stdb:reducer
func insertVecEveryPrimitiveStruct(_ server.ReducerContext, s []EveryPrimitiveStruct) {
	VecEveryPrimitiveStructTable.Insert(VecEveryPrimitiveStruct{S: s})
}

//stdb:reducer
func insertVecEveryVecStruct(_ server.ReducerContext, s []EveryVecStruct) {
	VecEveryVecStructTable.Insert(VecEveryVecStruct{S: s})
}

// ---------------------------------------------------------------------------
// Option* insert reducers -- each inserts a row with an optional value.
// ---------------------------------------------------------------------------

//stdb:reducer
func insertOptionI32(_ server.ReducerContext, n *int32) {
	OptionI32Table.Insert(OptionI32{N: n})
}

//stdb:reducer
func insertOptionString(_ server.ReducerContext, s *string) {
	OptionStringTable.Insert(OptionString{S: s})
}

//stdb:reducer
func insertOptionIdentity(_ server.ReducerContext, i *types.Identity) {
	OptionIdentityTable.Insert(OptionIdentity{I: i})
}

//stdb:reducer
func insertOptionUuid(_ server.ReducerContext, u *types.Uuid) {
	OptionUuidTable.Insert(OptionUuid{U: u})
}

//stdb:reducer
func insertOptionSimpleEnum(_ server.ReducerContext, e *SimpleEnum) {
	OptionSimpleEnumTable.Insert(OptionSimpleEnum{E: e})
}

//stdb:reducer
func insertOptionEveryPrimitiveStruct(_ server.ReducerContext, s *EveryPrimitiveStruct) {
	OptionEveryPrimitiveStructTable.Insert(OptionEveryPrimitiveStruct{S: s})
}

//stdb:reducer
func insertOptionVecOptionI32(_ server.ReducerContext, v *[]*int32) {
	OptionVecOptionI32Table.Insert(OptionVecOptionI32{V: v})
}

// ---------------------------------------------------------------------------
// Result* insert reducers
// ---------------------------------------------------------------------------

//stdb:reducer
func insertResultI32String(_ server.ReducerContext, r ResultI32StringValue) {
	ResultI32StringTable.Insert(ResultI32String{R: r})
}

//stdb:reducer
func insertResultStringI32(_ server.ReducerContext, r ResultStringI32Value) {
	ResultStringI32Table.Insert(ResultStringI32{R: r})
}

//stdb:reducer
func insertResultIdentityString(_ server.ReducerContext, r ResultIdentityStringValue) {
	ResultIdentityStringTable.Insert(ResultIdentityString{R: r})
}

//stdb:reducer
func insertResultSimpleEnumI32(_ server.ReducerContext, r ResultSimpleEnumI32Value) {
	ResultSimpleEnumI32Table.Insert(ResultSimpleEnumI32{R: r})
}

//stdb:reducer
func insertResultEveryPrimitiveStructString(_ server.ReducerContext, r ResultEveryPrimitiveStructStringValue) {
	ResultEveryPrimitiveStructStringTable.Insert(ResultEveryPrimitiveStructString{R: r})
}

//stdb:reducer
func insertResultVecI32String(_ server.ReducerContext, r ResultVecI32StringValue) {
	ResultVecI32StringTable.Insert(ResultVecI32String{R: r})
}

// ---------------------------------------------------------------------------
// Unique* CRUD reducers -- insert, update (delete+insert), delete by unique field.
// ---------------------------------------------------------------------------

//stdb:reducer
func insertUniqueU8(_ server.ReducerContext, n uint8, data int32) {
	UniqueU8Table.Insert(UniqueU8{N: n, Data: data})
}

//stdb:reducer
func updateUniqueU8(_ server.ReducerContext, n uint8, data int32) {
	UniqueU8Table.DeleteByN(n)
	UniqueU8Table.Insert(UniqueU8{N: n, Data: data})
}

//stdb:reducer
func deleteUniqueU8(_ server.ReducerContext, n uint8) {
	UniqueU8Table.DeleteByN(n)
}

//stdb:reducer
func insertUniqueU16(_ server.ReducerContext, n uint16, data int32) {
	UniqueU16Table.Insert(UniqueU16{N: n, Data: data})
}

//stdb:reducer
func updateUniqueU16(_ server.ReducerContext, n uint16, data int32) {
	UniqueU16Table.DeleteByN(n)
	UniqueU16Table.Insert(UniqueU16{N: n, Data: data})
}

//stdb:reducer
func deleteUniqueU16(_ server.ReducerContext, n uint16) {
	UniqueU16Table.DeleteByN(n)
}

//stdb:reducer
func insertUniqueU32(_ server.ReducerContext, n uint32, data int32) {
	UniqueU32Table.Insert(UniqueU32{N: n, Data: data})
}

//stdb:reducer
func updateUniqueU32(_ server.ReducerContext, n uint32, data int32) {
	UniqueU32Table.DeleteByN(n)
	UniqueU32Table.Insert(UniqueU32{N: n, Data: data})
}

//stdb:reducer
func deleteUniqueU32(_ server.ReducerContext, n uint32) {
	UniqueU32Table.DeleteByN(n)
}

//stdb:reducer
func insertUniqueU64(_ server.ReducerContext, n uint64, data int32) {
	UniqueU64Table.Insert(UniqueU64{N: n, Data: data})
}

//stdb:reducer
func updateUniqueU64(_ server.ReducerContext, n uint64, data int32) {
	UniqueU64Table.DeleteByN(n)
	UniqueU64Table.Insert(UniqueU64{N: n, Data: data})
}

//stdb:reducer
func deleteUniqueU64(_ server.ReducerContext, n uint64) {
	UniqueU64Table.DeleteByN(n)
}

//stdb:reducer
func insertUniqueU128(_ server.ReducerContext, n types.Uint128, data int32) {
	UniqueU128Table.Insert(UniqueU128{N: n, Data: data})
}

//stdb:reducer
func updateUniqueU128(_ server.ReducerContext, n types.Uint128, data int32) {
	UniqueU128Table.DeleteByN(n)
	UniqueU128Table.Insert(UniqueU128{N: n, Data: data})
}

//stdb:reducer
func deleteUniqueU128(_ server.ReducerContext, n types.Uint128) {
	UniqueU128Table.DeleteByN(n)
}

//stdb:reducer
func insertUniqueU256(_ server.ReducerContext, n types.Uint256, data int32) {
	UniqueU256Table.Insert(UniqueU256{N: n, Data: data})
}

//stdb:reducer
func updateUniqueU256(_ server.ReducerContext, n types.Uint256, data int32) {
	UniqueU256Table.DeleteByN(n)
	UniqueU256Table.Insert(UniqueU256{N: n, Data: data})
}

//stdb:reducer
func deleteUniqueU256(_ server.ReducerContext, n types.Uint256) {
	UniqueU256Table.DeleteByN(n)
}

//stdb:reducer
func insertUniqueI8(_ server.ReducerContext, n int8, data int32) {
	UniqueI8Table.Insert(UniqueI8{N: n, Data: data})
}

//stdb:reducer
func updateUniqueI8(_ server.ReducerContext, n int8, data int32) {
	UniqueI8Table.DeleteByN(n)
	UniqueI8Table.Insert(UniqueI8{N: n, Data: data})
}

//stdb:reducer
func deleteUniqueI8(_ server.ReducerContext, n int8) {
	UniqueI8Table.DeleteByN(n)
}

//stdb:reducer
func insertUniqueI16(_ server.ReducerContext, n int16, data int32) {
	UniqueI16Table.Insert(UniqueI16{N: n, Data: data})
}

//stdb:reducer
func updateUniqueI16(_ server.ReducerContext, n int16, data int32) {
	UniqueI16Table.DeleteByN(n)
	UniqueI16Table.Insert(UniqueI16{N: n, Data: data})
}

//stdb:reducer
func deleteUniqueI16(_ server.ReducerContext, n int16) {
	UniqueI16Table.DeleteByN(n)
}

//stdb:reducer
func insertUniqueI32(_ server.ReducerContext, n int32, data int32) {
	UniqueI32Table.Insert(UniqueI32{N: n, Data: data})
}

//stdb:reducer
func updateUniqueI32(_ server.ReducerContext, n int32, data int32) {
	UniqueI32Table.DeleteByN(n)
	UniqueI32Table.Insert(UniqueI32{N: n, Data: data})
}

//stdb:reducer
func deleteUniqueI32(_ server.ReducerContext, n int32) {
	UniqueI32Table.DeleteByN(n)
}

//stdb:reducer
func insertUniqueI64(_ server.ReducerContext, n int64, data int32) {
	UniqueI64Table.Insert(UniqueI64{N: n, Data: data})
}

//stdb:reducer
func updateUniqueI64(_ server.ReducerContext, n int64, data int32) {
	UniqueI64Table.DeleteByN(n)
	UniqueI64Table.Insert(UniqueI64{N: n, Data: data})
}

//stdb:reducer
func deleteUniqueI64(_ server.ReducerContext, n int64) {
	UniqueI64Table.DeleteByN(n)
}

//stdb:reducer
func insertUniqueI128(_ server.ReducerContext, n types.Int128, data int32) {
	UniqueI128Table.Insert(UniqueI128{N: n, Data: data})
}

//stdb:reducer
func updateUniqueI128(_ server.ReducerContext, n types.Int128, data int32) {
	UniqueI128Table.DeleteByN(n)
	UniqueI128Table.Insert(UniqueI128{N: n, Data: data})
}

//stdb:reducer
func deleteUniqueI128(_ server.ReducerContext, n types.Int128) {
	UniqueI128Table.DeleteByN(n)
}

//stdb:reducer
func insertUniqueI256(_ server.ReducerContext, n types.Int256, data int32) {
	UniqueI256Table.Insert(UniqueI256{N: n, Data: data})
}

//stdb:reducer
func updateUniqueI256(_ server.ReducerContext, n types.Int256, data int32) {
	UniqueI256Table.DeleteByN(n)
	UniqueI256Table.Insert(UniqueI256{N: n, Data: data})
}

//stdb:reducer
func deleteUniqueI256(_ server.ReducerContext, n types.Int256) {
	UniqueI256Table.DeleteByN(n)
}

//stdb:reducer
func insertUniqueBool(_ server.ReducerContext, b bool, data int32) {
	UniqueBoolTable.Insert(UniqueBool{B: b, Data: data})
}

//stdb:reducer
func updateUniqueBool(_ server.ReducerContext, b bool, data int32) {
	UniqueBoolTable.DeleteByB(b)
	UniqueBoolTable.Insert(UniqueBool{B: b, Data: data})
}

//stdb:reducer
func deleteUniqueBool(_ server.ReducerContext, b bool) {
	UniqueBoolTable.DeleteByB(b)
}

//stdb:reducer
func insertUniqueString(_ server.ReducerContext, s string, data int32) {
	UniqueStringTable.Insert(UniqueString{S: s, Data: data})
}

//stdb:reducer
func updateUniqueString(_ server.ReducerContext, s string, data int32) {
	UniqueStringTable.DeleteByS(s)
	UniqueStringTable.Insert(UniqueString{S: s, Data: data})
}

//stdb:reducer
func deleteUniqueString(_ server.ReducerContext, s string) {
	UniqueStringTable.DeleteByS(s)
}

//stdb:reducer
func insertUniqueIdentity(_ server.ReducerContext, i types.Identity, data int32) {
	UniqueIdentityTable.Insert(UniqueIdentity{I: i, Data: data})
}

//stdb:reducer
func updateUniqueIdentity(_ server.ReducerContext, i types.Identity, data int32) {
	UniqueIdentityTable.DeleteByI(i)
	UniqueIdentityTable.Insert(UniqueIdentity{I: i, Data: data})
}

//stdb:reducer
func deleteUniqueIdentity(_ server.ReducerContext, i types.Identity) {
	UniqueIdentityTable.DeleteByI(i)
}

//stdb:reducer
func insertUniqueConnectionId(_ server.ReducerContext, a types.ConnectionId, data int32) {
	UniqueConnectionIdTable.Insert(UniqueConnectionId{A: a, Data: data})
}

//stdb:reducer
func updateUniqueConnectionId(_ server.ReducerContext, a types.ConnectionId, data int32) {
	UniqueConnectionIdTable.DeleteByA(a)
	UniqueConnectionIdTable.Insert(UniqueConnectionId{A: a, Data: data})
}

//stdb:reducer
func deleteUniqueConnectionId(_ server.ReducerContext, a types.ConnectionId) {
	UniqueConnectionIdTable.DeleteByA(a)
}

//stdb:reducer
func insertUniqueUuid(_ server.ReducerContext, u types.Uuid, data int32) {
	UniqueUuidTable.Insert(UniqueUuid{U: u, Data: data})
}

//stdb:reducer
func updateUniqueUuid(_ server.ReducerContext, u types.Uuid, data int32) {
	UniqueUuidTable.DeleteByU(u)
	UniqueUuidTable.Insert(UniqueUuid{U: u, Data: data})
}

//stdb:reducer
func deleteUniqueUuid(_ server.ReducerContext, u types.Uuid) {
	UniqueUuidTable.DeleteByU(u)
}

// ---------------------------------------------------------------------------
// Pk* CRUD reducers -- insert, update (UpdateBy), delete (DeleteBy) by PK.
// ---------------------------------------------------------------------------

//stdb:reducer
func insertPkU8(_ server.ReducerContext, n uint8, data int32) {
	PkU8Table.Insert(PkU8{N: n, Data: data})
}

//stdb:reducer
func updatePkU8(_ server.ReducerContext, n uint8, data int32) {
	PkU8Table.UpdateByN(PkU8{N: n, Data: data})
}

//stdb:reducer
func deletePkU8(_ server.ReducerContext, n uint8) {
	PkU8Table.DeleteByN(n)
}

//stdb:reducer
func insertPkU16(_ server.ReducerContext, n uint16, data int32) {
	PkU16Table.Insert(PkU16{N: n, Data: data})
}

//stdb:reducer
func updatePkU16(_ server.ReducerContext, n uint16, data int32) {
	PkU16Table.UpdateByN(PkU16{N: n, Data: data})
}

//stdb:reducer
func deletePkU16(_ server.ReducerContext, n uint16) {
	PkU16Table.DeleteByN(n)
}

//stdb:reducer
func insertPkU32(_ server.ReducerContext, n uint32, data int32) {
	PkU32Table.Insert(PkU32{N: n, Data: data})
}

//stdb:reducer
func updatePkU32(_ server.ReducerContext, n uint32, data int32) {
	PkU32Table.UpdateByN(PkU32{N: n, Data: data})
}

//stdb:reducer
func deletePkU32(_ server.ReducerContext, n uint32) {
	PkU32Table.DeleteByN(n)
}

//stdb:reducer
func insertPkU64(_ server.ReducerContext, n uint64, data int32) {
	PkU64Table.Insert(PkU64{N: n, Data: data})
}

//stdb:reducer
func updatePkU64(_ server.ReducerContext, n uint64, data int32) {
	PkU64Table.UpdateByN(PkU64{N: n, Data: data})
}

//stdb:reducer
func deletePkU64(_ server.ReducerContext, n uint64) {
	PkU64Table.DeleteByN(n)
}

//stdb:reducer
func insertPkU128(_ server.ReducerContext, n types.Uint128, data int32) {
	PkU128Table.Insert(PkU128{N: n, Data: data})
}

//stdb:reducer
func updatePkU128(_ server.ReducerContext, n types.Uint128, data int32) {
	PkU128Table.UpdateByN(PkU128{N: n, Data: data})
}

//stdb:reducer
func deletePkU128(_ server.ReducerContext, n types.Uint128) {
	PkU128Table.DeleteByN(n)
}

//stdb:reducer
func insertPkU256(_ server.ReducerContext, n types.Uint256, data int32) {
	PkU256Table.Insert(PkU256{N: n, Data: data})
}

//stdb:reducer
func updatePkU256(_ server.ReducerContext, n types.Uint256, data int32) {
	PkU256Table.UpdateByN(PkU256{N: n, Data: data})
}

//stdb:reducer
func deletePkU256(_ server.ReducerContext, n types.Uint256) {
	PkU256Table.DeleteByN(n)
}

//stdb:reducer
func insertPkI8(_ server.ReducerContext, n int8, data int32) {
	PkI8Table.Insert(PkI8{N: n, Data: data})
}

//stdb:reducer
func updatePkI8(_ server.ReducerContext, n int8, data int32) {
	PkI8Table.UpdateByN(PkI8{N: n, Data: data})
}

//stdb:reducer
func deletePkI8(_ server.ReducerContext, n int8) {
	PkI8Table.DeleteByN(n)
}

//stdb:reducer
func insertPkI16(_ server.ReducerContext, n int16, data int32) {
	PkI16Table.Insert(PkI16{N: n, Data: data})
}

//stdb:reducer
func updatePkI16(_ server.ReducerContext, n int16, data int32) {
	PkI16Table.UpdateByN(PkI16{N: n, Data: data})
}

//stdb:reducer
func deletePkI16(_ server.ReducerContext, n int16) {
	PkI16Table.DeleteByN(n)
}

//stdb:reducer
func insertPkI32(_ server.ReducerContext, n int32, data int32) {
	PkI32Table.Insert(PkI32{N: n, Data: data})
}

//stdb:reducer
func updatePkI32(_ server.ReducerContext, n int32, data int32) {
	PkI32Table.UpdateByN(PkI32{N: n, Data: data})
}

//stdb:reducer
func deletePkI32(_ server.ReducerContext, n int32) {
	PkI32Table.DeleteByN(n)
}

//stdb:reducer
func insertPkI64(_ server.ReducerContext, n int64, data int32) {
	PkI64Table.Insert(PkI64{N: n, Data: data})
}

//stdb:reducer
func updatePkI64(_ server.ReducerContext, n int64, data int32) {
	PkI64Table.UpdateByN(PkI64{N: n, Data: data})
}

//stdb:reducer
func deletePkI64(_ server.ReducerContext, n int64) {
	PkI64Table.DeleteByN(n)
}

//stdb:reducer
func insertPkI128(_ server.ReducerContext, n types.Int128, data int32) {
	PkI128Table.Insert(PkI128{N: n, Data: data})
}

//stdb:reducer
func updatePkI128(_ server.ReducerContext, n types.Int128, data int32) {
	PkI128Table.UpdateByN(PkI128{N: n, Data: data})
}

//stdb:reducer
func deletePkI128(_ server.ReducerContext, n types.Int128) {
	PkI128Table.DeleteByN(n)
}

//stdb:reducer
func insertPkI256(_ server.ReducerContext, n types.Int256, data int32) {
	PkI256Table.Insert(PkI256{N: n, Data: data})
}

//stdb:reducer
func updatePkI256(_ server.ReducerContext, n types.Int256, data int32) {
	PkI256Table.UpdateByN(PkI256{N: n, Data: data})
}

//stdb:reducer
func deletePkI256(_ server.ReducerContext, n types.Int256) {
	PkI256Table.DeleteByN(n)
}

//stdb:reducer
func insertPkBool(_ server.ReducerContext, b bool, data int32) {
	PkBoolTable.Insert(PkBool{B: b, Data: data})
}

//stdb:reducer
func updatePkBool(_ server.ReducerContext, b bool, data int32) {
	PkBoolTable.UpdateByB(PkBool{B: b, Data: data})
}

//stdb:reducer
func deletePkBool(_ server.ReducerContext, b bool) {
	PkBoolTable.DeleteByB(b)
}

//stdb:reducer
func insertPkString(_ server.ReducerContext, s string, data int32) {
	PkStringTable.Insert(PkString{S: s, Data: data})
}

//stdb:reducer
func updatePkString(_ server.ReducerContext, s string, data int32) {
	PkStringTable.UpdateByS(PkString{S: s, Data: data})
}

//stdb:reducer
func deletePkString(_ server.ReducerContext, s string) {
	PkStringTable.DeleteByS(s)
}

//stdb:reducer
func insertPkIdentity(_ server.ReducerContext, i types.Identity, data int32) {
	PkIdentityTable.Insert(PkIdentity{I: i, Data: data})
}

//stdb:reducer
func updatePkIdentity(_ server.ReducerContext, i types.Identity, data int32) {
	PkIdentityTable.UpdateByI(PkIdentity{I: i, Data: data})
}

//stdb:reducer
func deletePkIdentity(_ server.ReducerContext, i types.Identity) {
	PkIdentityTable.DeleteByI(i)
}

//stdb:reducer
func insertPkConnectionId(_ server.ReducerContext, a types.ConnectionId, data int32) {
	PkConnectionIdTable.Insert(PkConnectionId{A: a, Data: data})
}

//stdb:reducer
func updatePkConnectionId(_ server.ReducerContext, a types.ConnectionId, data int32) {
	PkConnectionIdTable.UpdateByA(PkConnectionId{A: a, Data: data})
}

//stdb:reducer
func deletePkConnectionId(_ server.ReducerContext, a types.ConnectionId) {
	PkConnectionIdTable.DeleteByA(a)
}

//stdb:reducer
func insertPkUuid(_ server.ReducerContext, u types.Uuid, data int32) {
	PkUuidTable.Insert(PkUuid{U: u, Data: data})
}

//stdb:reducer
func updatePkUuid(_ server.ReducerContext, u types.Uuid, data int32) {
	PkUuidTable.UpdateByU(PkUuid{U: u, Data: data})
}

//stdb:reducer
func deletePkUuid(_ server.ReducerContext, u types.Uuid) {
	PkUuidTable.DeleteByU(u)
}

//stdb:reducer
func insertPkSimpleEnum(_ server.ReducerContext, a SimpleEnum, data int32) {
	PkSimpleEnumTable.Insert(PkSimpleEnum{A: a, Data: data})
}

//stdb:reducer
func insertPkU32Two(_ server.ReducerContext, n uint32, data int32) {
	PkU32TwoTable.Insert(PkU32Two{N: n, Data: data})
}

//stdb:reducer
func updatePkU32Two(_ server.ReducerContext, n uint32, data int32) {
	PkU32TwoTable.UpdateByN(PkU32Two{N: n, Data: data})
}

//stdb:reducer
func deletePkU32Two(_ server.ReducerContext, n uint32) {
	PkU32TwoTable.DeleteByN(n)
}

// ---------------------------------------------------------------------------
// Special reducers
// ---------------------------------------------------------------------------

//stdb:reducer
func updatePkSimpleEnum(_ server.ReducerContext, a SimpleEnum, data int32) error {
	_, found, err := PkSimpleEnumTable.FindByA(a)
	if err != nil {
		return err
	}
	if !found {
		return fmt.Errorf("row not found")
	}
	PkSimpleEnumTable.UpdateByA(PkSimpleEnum{A: a, Data: data})
	return nil
}

//stdb:reducer
func insertLargeTable(_ server.ReducerContext, a uint8, b uint16, c uint32, d uint64, e types.Uint128, f types.Uint256, g int8, h int16, i int32, j int64, k types.Int128, l types.Int256, m bool, n float32, o float64, p string, q SimpleEnum, r EnumWithPayload, s UnitStruct, t ByteStruct, u EveryPrimitiveStruct, v EveryVecStruct) {
	LargeTableTable.Insert(LargeTable{A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q, R: r, S: s, T: t, U: u, V: v})
}

//stdb:reducer
func deleteLargeTable(_ server.ReducerContext, a uint8, b uint16, c uint32, d uint64, e types.Uint128, f types.Uint256, g int8, h int16, i int32, j int64, k types.Int128, l types.Int256, m bool, n float32, o float64, p string, q SimpleEnum, r EnumWithPayload, s UnitStruct, t ByteStruct, u EveryPrimitiveStruct, v EveryVecStruct) {
	LargeTableTable.Delete(LargeTable{A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q, R: r, S: s, T: t, U: u, V: v})
}

//stdb:reducer
func insertTableHoldsTable(_ server.ReducerContext, a OneU8, b VecU8) {
	TableHoldsTableTable.Insert(TableHoldsTable{A: a, B: b})
}

//stdb:reducer
func insertIntoBtreeU32(_ server.ReducerContext, rows []BTreeU32) {
	for _, row := range rows {
		BtreeU32Table.Insert(row)
	}
}

//stdb:reducer
func deleteFromBtreeU32(_ server.ReducerContext, rows []BTreeU32) {
	for _, row := range rows {
		BtreeU32Table.Delete(row)
	}
}

//stdb:reducer
func insertIntoPkBtreeU32(_ server.ReducerContext, pkU32 []PkU32, btU32 []BTreeU32) {
	for _, row := range pkU32 {
		PkU32Table.Insert(row)
	}
	for _, row := range btU32 {
		BtreeU32Table.Insert(row)
	}
}

//stdb:reducer
func insertUniqueU32UpdatePkU32(_ server.ReducerContext, n uint32, dUnique int32, dPk int32) {
	UniqueU32Table.Insert(UniqueU32{N: n, Data: dUnique})
	PkU32Table.UpdateByN(PkU32{N: n, Data: dPk})
}

//stdb:reducer
func deletePkU32InsertPkU32Two(_ server.ReducerContext, n uint32, data int32) {
	PkU32TwoTable.Insert(PkU32Two{N: n, Data: data})
	PkU32Table.Delete(PkU32{N: n, Data: data})
}

// ---------------------------------------------------------------------------
// Caller identity/connection reducers -- use ctx.Sender() and ctx.ConnectionId().
// ---------------------------------------------------------------------------

//stdb:reducer
func insertCallerOneIdentity(ctx server.ReducerContext) {
	OneIdentityTable.Insert(OneIdentity{I: ctx.Sender()})
}

//stdb:reducer
func insertCallerVecIdentity(ctx server.ReducerContext) {
	VecIdentityTable.Insert(VecIdentity{I: []types.Identity{ctx.Sender()}})
}

//stdb:reducer
func insertCallerUniqueIdentity(ctx server.ReducerContext, data int32) {
	UniqueIdentityTable.Insert(UniqueIdentity{I: ctx.Sender(), Data: data})
}

//stdb:reducer
func insertCallerPkIdentity(ctx server.ReducerContext, data int32) {
	PkIdentityTable.Insert(PkIdentity{I: ctx.Sender(), Data: data})
}

//stdb:reducer
func insertCallerOneConnectionId(ctx server.ReducerContext) {
	OneConnectionIdTable.Insert(OneConnectionId{A: ctx.ConnectionId()})
}

//stdb:reducer
func insertCallerVecConnectionId(ctx server.ReducerContext) {
	VecConnectionIdTable.Insert(VecConnectionId{A: []types.ConnectionId{ctx.ConnectionId()}})
}

//stdb:reducer
func insertCallerUniqueConnectionId(ctx server.ReducerContext, data int32) {
	UniqueConnectionIdTable.Insert(UniqueConnectionId{A: ctx.ConnectionId(), Data: data})
}

//stdb:reducer
func insertCallerPkConnectionId(ctx server.ReducerContext, data int32) {
	PkConnectionIdTable.Insert(PkConnectionId{A: ctx.ConnectionId(), Data: data})
}

// ---------------------------------------------------------------------------
// Timestamp and UUID reducers
// ---------------------------------------------------------------------------

//stdb:reducer
func insertCallTimestamp(ctx server.ReducerContext) {
	OneTimestampTable.Insert(OneTimestamp{T: ctx.Timestamp()})
}

//stdb:reducer
func insertCallUuidV4(ctx server.ReducerContext) {
	ts := ctx.Timestamp()
	var b [16]byte
	usec := ts.Microseconds()
	b[0] = byte(usec)
	b[1] = byte(usec >> 8)
	b[2] = byte(usec >> 16)
	b[3] = byte(usec >> 24)
	b[4] = byte(usec >> 32)
	b[5] = byte(usec >> 40)
	b[6] = byte(usec >> 48)
	b[7] = byte(usec >> 56)
	b[6] = (b[6] & 0x0f) | 0x40
	b[8] = (b[8] & 0x3f) | 0x80
	OneUuidTable.Insert(OneUuid{U: types.NewUuid(b)})
}

//stdb:reducer
func insertCallUuidV7(ctx server.ReducerContext) {
	ts := ctx.Timestamp()
	var b [16]byte
	usec := ts.Microseconds()
	msec := usec / 1000
	b[0] = byte(msec >> 40)
	b[1] = byte(msec >> 32)
	b[2] = byte(msec >> 24)
	b[3] = byte(msec >> 16)
	b[4] = byte(msec >> 8)
	b[5] = byte(msec)
	b[6] = (b[6] & 0x0f) | 0x70
	b[8] = (b[8] & 0x3f) | 0x80
	OneUuidTable.Insert(OneUuid{U: types.NewUuid(b)})
}

// ---------------------------------------------------------------------------
// insertPrimitivesAsStrings converts each field of EveryPrimitiveStruct to a
// string and inserts the result into VecString.
// ---------------------------------------------------------------------------

//stdb:reducer
func insertPrimitivesAsStrings(_ server.ReducerContext, s EveryPrimitiveStruct) {
	VecStringTable.Insert(VecString{S: []string{
		fmt.Sprintf("%d", s.A),
		fmt.Sprintf("%d", s.B),
		fmt.Sprintf("%d", s.C),
		fmt.Sprintf("%d", s.D),
		s.E.String(),
		s.F.String(),
		fmt.Sprintf("%d", s.G),
		fmt.Sprintf("%d", s.H),
		fmt.Sprintf("%d", s.I),
		fmt.Sprintf("%d", s.J),
		s.K.String(),
		s.L.String(),
		fmt.Sprintf("%t", s.M),
		fmt.Sprintf("%g", s.N),
		fmt.Sprintf("%g", s.O),
		s.P,
		s.Q.String(),
		s.R.String(),
		s.S.String(),
		s.T.String(),
		s.U.String(),
	}})
}

// ---------------------------------------------------------------------------
// Misc reducers
// ---------------------------------------------------------------------------

//stdb:reducer
func noOpSucceeds(_ server.ReducerContext) {}

//stdb:reducer
func sendScheduledMessage(_ server.ReducerContext, arg ScheduledTable) {
	_ = arg.Text
	_ = arg.ScheduledAt
	_ = arg.ScheduledId
}

//stdb:reducer
func insertUser(_ server.ReducerContext, name string, identity types.Identity) {
	UsersTable.Insert(Users{Identity: identity, Name: name})
}

//stdb:reducer
func insertIntoIndexedSimpleEnum(_ server.ReducerContext, n SimpleEnum) {
	IndexedSimpleEnumTable.Insert(IndexedSimpleEnum{N: n})
}

//stdb:reducer
func updateIndexedSimpleEnum(_ server.ReducerContext, a SimpleEnum, b SimpleEnum) error {
	iter, err := IndexedSimpleEnumTable.Scan()
	if err != nil {
		return err
	}
	defer iter.Close()
	found := false
	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		if row.N == a {
			IndexedSimpleEnumTable.Delete(row)
			found = true
			break
		}
	}
	if found {
		IndexedSimpleEnumTable.Insert(IndexedSimpleEnum{N: b})
	}
	return nil
}

//stdb:reducer
func sortedUuidsInsert(ctx server.ReducerContext) error {
	ts := ctx.Timestamp()
	usec := ts.Microseconds()
	msec := usec / 1000

	for i := 0; i < 1000; i++ {
		var b [16]byte
		b[0] = byte(msec >> 40)
		b[1] = byte(msec >> 32)
		b[2] = byte(msec >> 24)
		b[3] = byte(msec >> 16)
		b[4] = byte(msec >> 8)
		b[5] = byte(msec)
		counter := uint16(i)
		b[6] = byte(counter>>8) | 0x70
		b[7] = byte(counter)
		b[8] = 0x80
		b[9] = byte(i)

		uuid := types.NewUuid(b)
		PkUuidTable.Insert(PkUuid{U: uuid, Data: 0})
	}

	iter, err := PkUuidTable.Scan()
	if err != nil {
		return err
	}
	defer iter.Close()

	var lastUuid types.Uuid
	first := true
	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		if !first {
			lastBytes := lastUuid.Bytes()
			curBytes := row.U.Bytes()
			for idx := 0; idx < 16; idx++ {
				if lastBytes[idx] < curBytes[idx] {
					break
				}
				if lastBytes[idx] > curBytes[idx] {
					return fmt.Errorf("UUIDs are not sorted correctly")
				}
			}
		}
		lastUuid = row.U
		first = false
	}

	return nil
}
