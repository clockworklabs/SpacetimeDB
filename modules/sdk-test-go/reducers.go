package main

import (
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/runtime"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// Reducer registrations are in init.go to ensure correct ordering.

// ---------------------------------------------------------------------------
// One* insert reducers -- each inserts a single-field row.
// ---------------------------------------------------------------------------

func insertOneU8(_ server.ReducerContext, n uint8) {
	runtime.Insert(OneU8{N: n})
}

func insertOneU16(_ server.ReducerContext, n uint16) {
	runtime.Insert(OneU16{N: n})
}

func insertOneU32(_ server.ReducerContext, n uint32) {
	runtime.Insert(OneU32{N: n})
}

func insertOneU64(_ server.ReducerContext, n uint64) {
	runtime.Insert(OneU64{N: n})
}

func insertOneU128(_ server.ReducerContext, n types.Uint128) {
	runtime.Insert(OneU128{N: n})
}

func insertOneU256(_ server.ReducerContext, n types.Uint256) {
	runtime.Insert(OneU256{N: n})
}

func insertOneI8(_ server.ReducerContext, n int8) {
	runtime.Insert(OneI8{N: n})
}

func insertOneI16(_ server.ReducerContext, n int16) {
	runtime.Insert(OneI16{N: n})
}

func insertOneI32(_ server.ReducerContext, n int32) {
	runtime.Insert(OneI32{N: n})
}

func insertOneI64(_ server.ReducerContext, n int64) {
	runtime.Insert(OneI64{N: n})
}

func insertOneI128(_ server.ReducerContext, n types.Int128) {
	runtime.Insert(OneI128{N: n})
}

func insertOneI256(_ server.ReducerContext, n types.Int256) {
	runtime.Insert(OneI256{N: n})
}

func insertOneBool(_ server.ReducerContext, b bool) {
	runtime.Insert(OneBool{B: b})
}

func insertOneF32(_ server.ReducerContext, f float32) {
	runtime.Insert(OneF32{F: f})
}

func insertOneF64(_ server.ReducerContext, f float64) {
	runtime.Insert(OneF64{F: f})
}

func insertOneString(_ server.ReducerContext, s string) {
	runtime.Insert(OneString{S: s})
}

func insertOneIdentity(_ server.ReducerContext, i types.Identity) {
	runtime.Insert(OneIdentity{I: i})
}

func insertOneConnectionId(_ server.ReducerContext, a types.ConnectionId) {
	runtime.Insert(OneConnectionId{A: a})
}

func insertOneUuid(_ server.ReducerContext, u types.Uuid) {
	runtime.Insert(OneUuid{U: u})
}

func insertOneTimestamp(_ server.ReducerContext, t types.Timestamp) {
	runtime.Insert(OneTimestamp{T: t})
}

func insertOneSimpleEnum(_ server.ReducerContext, e SimpleEnum) {
	runtime.Insert(OneSimpleEnum{E: e})
}

func insertOneEnumWithPayload(_ server.ReducerContext, e EnumWithPayload) {
	runtime.Insert(OneEnumWithPayload{E: e})
}

func insertOneUnitStruct(_ server.ReducerContext, s UnitStruct) {
	runtime.Insert(OneUnitStruct{S: s})
}

func insertOneByteStruct(_ server.ReducerContext, s ByteStruct) {
	runtime.Insert(OneByteStruct{S: s})
}

func insertOneEveryPrimitiveStruct(_ server.ReducerContext, s EveryPrimitiveStruct) {
	runtime.Insert(OneEveryPrimitiveStruct{S: s})
}

func insertOneEveryVecStruct(_ server.ReducerContext, s EveryVecStruct) {
	runtime.Insert(OneEveryVecStruct{S: s})
}

// ---------------------------------------------------------------------------
// Vec* insert reducers -- each inserts a row containing a slice.
// ---------------------------------------------------------------------------

func insertVecU8(_ server.ReducerContext, n []uint8) {
	runtime.Insert(VecU8{N: n})
}

func insertVecU16(_ server.ReducerContext, n []uint16) {
	runtime.Insert(VecU16{N: n})
}

func insertVecU32(_ server.ReducerContext, n []uint32) {
	runtime.Insert(VecU32{N: n})
}

func insertVecU64(_ server.ReducerContext, n []uint64) {
	runtime.Insert(VecU64{N: n})
}

func insertVecU128(_ server.ReducerContext, n []types.Uint128) {
	runtime.Insert(VecU128{N: n})
}

func insertVecU256(_ server.ReducerContext, n []types.Uint256) {
	runtime.Insert(VecU256{N: n})
}

func insertVecI8(_ server.ReducerContext, n []int8) {
	runtime.Insert(VecI8{N: n})
}

func insertVecI16(_ server.ReducerContext, n []int16) {
	runtime.Insert(VecI16{N: n})
}

func insertVecI32(_ server.ReducerContext, n []int32) {
	runtime.Insert(VecI32{N: n})
}

func insertVecI64(_ server.ReducerContext, n []int64) {
	runtime.Insert(VecI64{N: n})
}

func insertVecI128(_ server.ReducerContext, n []types.Int128) {
	runtime.Insert(VecI128{N: n})
}

func insertVecI256(_ server.ReducerContext, n []types.Int256) {
	runtime.Insert(VecI256{N: n})
}

func insertVecBool(_ server.ReducerContext, b []bool) {
	runtime.Insert(VecBool{B: b})
}

func insertVecF32(_ server.ReducerContext, f []float32) {
	runtime.Insert(VecF32{F: f})
}

func insertVecF64(_ server.ReducerContext, f []float64) {
	runtime.Insert(VecF64{F: f})
}

func insertVecString(_ server.ReducerContext, s []string) {
	runtime.Insert(VecString{S: s})
}

func insertVecIdentity(_ server.ReducerContext, i []types.Identity) {
	runtime.Insert(VecIdentity{I: i})
}

func insertVecConnectionId(_ server.ReducerContext, a []types.ConnectionId) {
	runtime.Insert(VecConnectionId{A: a})
}

func insertVecUuid(_ server.ReducerContext, u []types.Uuid) {
	runtime.Insert(VecUuid{U: u})
}

func insertVecTimestamp(_ server.ReducerContext, t []types.Timestamp) {
	runtime.Insert(VecTimestamp{T: t})
}

func insertVecSimpleEnum(_ server.ReducerContext, e []SimpleEnum) {
	runtime.Insert(VecSimpleEnum{E: e})
}

func insertVecEnumWithPayload(_ server.ReducerContext, e []EnumWithPayload) {
	runtime.Insert(VecEnumWithPayload{E: e})
}

func insertVecUnitStruct(_ server.ReducerContext, s []UnitStruct) {
	runtime.Insert(VecUnitStruct{S: s})
}

func insertVecByteStruct(_ server.ReducerContext, s []ByteStruct) {
	runtime.Insert(VecByteStruct{S: s})
}

func insertVecEveryPrimitiveStruct(_ server.ReducerContext, s []EveryPrimitiveStruct) {
	runtime.Insert(VecEveryPrimitiveStruct{S: s})
}

func insertVecEveryVecStruct(_ server.ReducerContext, s []EveryVecStruct) {
	runtime.Insert(VecEveryVecStruct{S: s})
}

// ---------------------------------------------------------------------------
// Option* insert reducers -- each inserts a row with an optional value.
// ---------------------------------------------------------------------------

func insertOptionI32(_ server.ReducerContext, n *int32) {
	runtime.Insert(OptionI32{N: n})
}

func insertOptionString(_ server.ReducerContext, s *string) {
	runtime.Insert(OptionString{S: s})
}

func insertOptionIdentity(_ server.ReducerContext, i *types.Identity) {
	runtime.Insert(OptionIdentity{I: i})
}

func insertOptionUuid(_ server.ReducerContext, u *types.Uuid) {
	runtime.Insert(OptionUuid{U: u})
}

func insertOptionSimpleEnum(_ server.ReducerContext, e *SimpleEnum) {
	runtime.Insert(OptionSimpleEnum{E: e})
}

func insertOptionEveryPrimitiveStruct(_ server.ReducerContext, s *EveryPrimitiveStruct) {
	runtime.Insert(OptionEveryPrimitiveStruct{S: s})
}

func insertOptionVecOptionI32(_ server.ReducerContext, v *[](*int32)) {
	runtime.Insert(OptionVecOptionI32{V: v})
}

// ---------------------------------------------------------------------------
// Result* insert reducers
// ---------------------------------------------------------------------------

func insertResultI32String(_ server.ReducerContext, r ResultI32StringValue) {
	runtime.Insert(ResultI32String{R: r})
}

func insertResultStringI32(_ server.ReducerContext, r ResultStringI32Value) {
	runtime.Insert(ResultStringI32{R: r})
}

func insertResultIdentityString(_ server.ReducerContext, r ResultIdentityStringValue) {
	runtime.Insert(ResultIdentityString{R: r})
}

func insertResultSimpleEnumI32(_ server.ReducerContext, r ResultSimpleEnumI32Value) {
	runtime.Insert(ResultSimpleEnumI32{R: r})
}

func insertResultEveryPrimitiveStructString(_ server.ReducerContext, r ResultEveryPrimitiveStructStringValue) {
	runtime.Insert(ResultEveryPrimitiveStructString{R: r})
}

func insertResultVecI32String(_ server.ReducerContext, r ResultVecI32StringValue) {
	runtime.Insert(ResultVecI32String{R: r})
}

// ---------------------------------------------------------------------------
// Unique* CRUD reducers -- insert, update (delete+insert), delete by unique field.
// ---------------------------------------------------------------------------

// --- UniqueU8 ---

func insertUniqueU8(_ server.ReducerContext, n uint8, data int32) {
	runtime.Insert(UniqueU8{N: n, Data: data})
}

func updateUniqueU8(_ server.ReducerContext, n uint8, data int32) {
	runtime.DeleteBy[UniqueU8, uint8]("unique_u_8_n_idx_btree", n)
	runtime.Insert(UniqueU8{N: n, Data: data})
}

func deleteUniqueU8(_ server.ReducerContext, n uint8) {
	runtime.DeleteBy[UniqueU8, uint8]("unique_u_8_n_idx_btree", n)
}

// --- UniqueU16 ---

func insertUniqueU16(_ server.ReducerContext, n uint16, data int32) {
	runtime.Insert(UniqueU16{N: n, Data: data})
}

func updateUniqueU16(_ server.ReducerContext, n uint16, data int32) {
	runtime.DeleteBy[UniqueU16, uint16]("unique_u_16_n_idx_btree", n)
	runtime.Insert(UniqueU16{N: n, Data: data})
}

func deleteUniqueU16(_ server.ReducerContext, n uint16) {
	runtime.DeleteBy[UniqueU16, uint16]("unique_u_16_n_idx_btree", n)
}

// --- UniqueU32 ---

func insertUniqueU32(_ server.ReducerContext, n uint32, data int32) {
	runtime.Insert(UniqueU32{N: n, Data: data})
}

func updateUniqueU32(_ server.ReducerContext, n uint32, data int32) {
	runtime.DeleteBy[UniqueU32, uint32]("unique_u_32_n_idx_btree", n)
	runtime.Insert(UniqueU32{N: n, Data: data})
}

func deleteUniqueU32(_ server.ReducerContext, n uint32) {
	runtime.DeleteBy[UniqueU32, uint32]("unique_u_32_n_idx_btree", n)
}

// --- UniqueU64 ---

func insertUniqueU64(_ server.ReducerContext, n uint64, data int32) {
	runtime.Insert(UniqueU64{N: n, Data: data})
}

func updateUniqueU64(_ server.ReducerContext, n uint64, data int32) {
	runtime.DeleteBy[UniqueU64, uint64]("unique_u_64_n_idx_btree", n)
	runtime.Insert(UniqueU64{N: n, Data: data})
}

func deleteUniqueU64(_ server.ReducerContext, n uint64) {
	runtime.DeleteBy[UniqueU64, uint64]("unique_u_64_n_idx_btree", n)
}

// --- UniqueU128 ---

func insertUniqueU128(_ server.ReducerContext, n types.Uint128, data int32) {
	runtime.Insert(UniqueU128{N: n, Data: data})
}

func updateUniqueU128(_ server.ReducerContext, n types.Uint128, data int32) {
	runtime.DeleteBy[UniqueU128, types.Uint128]("unique_u_128_n_idx_btree", n)
	runtime.Insert(UniqueU128{N: n, Data: data})
}

func deleteUniqueU128(_ server.ReducerContext, n types.Uint128) {
	runtime.DeleteBy[UniqueU128, types.Uint128]("unique_u_128_n_idx_btree", n)
}

// --- UniqueU256 ---

func insertUniqueU256(_ server.ReducerContext, n types.Uint256, data int32) {
	runtime.Insert(UniqueU256{N: n, Data: data})
}

func updateUniqueU256(_ server.ReducerContext, n types.Uint256, data int32) {
	runtime.DeleteBy[UniqueU256, types.Uint256]("unique_u_256_n_idx_btree", n)
	runtime.Insert(UniqueU256{N: n, Data: data})
}

func deleteUniqueU256(_ server.ReducerContext, n types.Uint256) {
	runtime.DeleteBy[UniqueU256, types.Uint256]("unique_u_256_n_idx_btree", n)
}

// --- UniqueI8 ---

func insertUniqueI8(_ server.ReducerContext, n int8, data int32) {
	runtime.Insert(UniqueI8{N: n, Data: data})
}

func updateUniqueI8(_ server.ReducerContext, n int8, data int32) {
	runtime.DeleteBy[UniqueI8, int8]("unique_i_8_n_idx_btree", n)
	runtime.Insert(UniqueI8{N: n, Data: data})
}

func deleteUniqueI8(_ server.ReducerContext, n int8) {
	runtime.DeleteBy[UniqueI8, int8]("unique_i_8_n_idx_btree", n)
}

// --- UniqueI16 ---

func insertUniqueI16(_ server.ReducerContext, n int16, data int32) {
	runtime.Insert(UniqueI16{N: n, Data: data})
}

func updateUniqueI16(_ server.ReducerContext, n int16, data int32) {
	runtime.DeleteBy[UniqueI16, int16]("unique_i_16_n_idx_btree", n)
	runtime.Insert(UniqueI16{N: n, Data: data})
}

func deleteUniqueI16(_ server.ReducerContext, n int16) {
	runtime.DeleteBy[UniqueI16, int16]("unique_i_16_n_idx_btree", n)
}

// --- UniqueI32 ---

func insertUniqueI32(_ server.ReducerContext, n int32, data int32) {
	runtime.Insert(UniqueI32{N: n, Data: data})
}

func updateUniqueI32(_ server.ReducerContext, n int32, data int32) {
	runtime.DeleteBy[UniqueI32, int32]("unique_i_32_n_idx_btree", n)
	runtime.Insert(UniqueI32{N: n, Data: data})
}

func deleteUniqueI32(_ server.ReducerContext, n int32) {
	runtime.DeleteBy[UniqueI32, int32]("unique_i_32_n_idx_btree", n)
}

// --- UniqueI64 ---

func insertUniqueI64(_ server.ReducerContext, n int64, data int32) {
	runtime.Insert(UniqueI64{N: n, Data: data})
}

func updateUniqueI64(_ server.ReducerContext, n int64, data int32) {
	runtime.DeleteBy[UniqueI64, int64]("unique_i_64_n_idx_btree", n)
	runtime.Insert(UniqueI64{N: n, Data: data})
}

func deleteUniqueI64(_ server.ReducerContext, n int64) {
	runtime.DeleteBy[UniqueI64, int64]("unique_i_64_n_idx_btree", n)
}

// --- UniqueI128 ---

func insertUniqueI128(_ server.ReducerContext, n types.Int128, data int32) {
	runtime.Insert(UniqueI128{N: n, Data: data})
}

func updateUniqueI128(_ server.ReducerContext, n types.Int128, data int32) {
	runtime.DeleteBy[UniqueI128, types.Int128]("unique_i_128_n_idx_btree", n)
	runtime.Insert(UniqueI128{N: n, Data: data})
}

func deleteUniqueI128(_ server.ReducerContext, n types.Int128) {
	runtime.DeleteBy[UniqueI128, types.Int128]("unique_i_128_n_idx_btree", n)
}

// --- UniqueI256 ---

func insertUniqueI256(_ server.ReducerContext, n types.Int256, data int32) {
	runtime.Insert(UniqueI256{N: n, Data: data})
}

func updateUniqueI256(_ server.ReducerContext, n types.Int256, data int32) {
	runtime.DeleteBy[UniqueI256, types.Int256]("unique_i_256_n_idx_btree", n)
	runtime.Insert(UniqueI256{N: n, Data: data})
}

func deleteUniqueI256(_ server.ReducerContext, n types.Int256) {
	runtime.DeleteBy[UniqueI256, types.Int256]("unique_i_256_n_idx_btree", n)
}

// --- UniqueBool ---

func insertUniqueBool(_ server.ReducerContext, b bool, data int32) {
	runtime.Insert(UniqueBool{B: b, Data: data})
}

func updateUniqueBool(_ server.ReducerContext, b bool, data int32) {
	runtime.DeleteBy[UniqueBool, bool]("unique_bool_b_idx_btree", b)
	runtime.Insert(UniqueBool{B: b, Data: data})
}

func deleteUniqueBool(_ server.ReducerContext, b bool) {
	runtime.DeleteBy[UniqueBool, bool]("unique_bool_b_idx_btree", b)
}

// --- UniqueString ---

func insertUniqueString(_ server.ReducerContext, s string, data int32) {
	runtime.Insert(UniqueString{S: s, Data: data})
}

func updateUniqueString(_ server.ReducerContext, s string, data int32) {
	runtime.DeleteBy[UniqueString, string]("unique_string_s_idx_btree", s)
	runtime.Insert(UniqueString{S: s, Data: data})
}

func deleteUniqueString(_ server.ReducerContext, s string) {
	runtime.DeleteBy[UniqueString, string]("unique_string_s_idx_btree", s)
}

// --- UniqueIdentity ---

func insertUniqueIdentity(_ server.ReducerContext, i types.Identity, data int32) {
	runtime.Insert(UniqueIdentity{I: i, Data: data})
}

func updateUniqueIdentity(_ server.ReducerContext, i types.Identity, data int32) {
	runtime.DeleteBy[UniqueIdentity, types.Identity]("unique_identity_i_idx_btree", i)
	runtime.Insert(UniqueIdentity{I: i, Data: data})
}

func deleteUniqueIdentity(_ server.ReducerContext, i types.Identity) {
	runtime.DeleteBy[UniqueIdentity, types.Identity]("unique_identity_i_idx_btree", i)
}

// --- UniqueConnectionId ---

func insertUniqueConnectionId(_ server.ReducerContext, a types.ConnectionId, data int32) {
	runtime.Insert(UniqueConnectionId{A: a, Data: data})
}

func updateUniqueConnectionId(_ server.ReducerContext, a types.ConnectionId, data int32) {
	runtime.DeleteBy[UniqueConnectionId, types.ConnectionId]("unique_connection_id_a_idx_btree", a)
	runtime.Insert(UniqueConnectionId{A: a, Data: data})
}

func deleteUniqueConnectionId(_ server.ReducerContext, a types.ConnectionId) {
	runtime.DeleteBy[UniqueConnectionId, types.ConnectionId]("unique_connection_id_a_idx_btree", a)
}

// --- UniqueUuid ---

func insertUniqueUuid(_ server.ReducerContext, u types.Uuid, data int32) {
	runtime.Insert(UniqueUuid{U: u, Data: data})
}

func updateUniqueUuid(_ server.ReducerContext, u types.Uuid, data int32) {
	runtime.DeleteBy[UniqueUuid, types.Uuid]("unique_uuid_u_idx_btree", u)
	runtime.Insert(UniqueUuid{U: u, Data: data})
}

func deleteUniqueUuid(_ server.ReducerContext, u types.Uuid) {
	runtime.DeleteBy[UniqueUuid, types.Uuid]("unique_uuid_u_idx_btree", u)
}

// ---------------------------------------------------------------------------
// Pk* CRUD reducers -- insert, update (UpdateBy), delete (DeleteBy) by PK.
// ---------------------------------------------------------------------------

// --- PkU8 ---

func insertPkU8(_ server.ReducerContext, n uint8, data int32) {
	runtime.Insert(PkU8{N: n, Data: data})
}

func updatePkU8(_ server.ReducerContext, n uint8, data int32) {
	runtime.UpdateBy[PkU8]("pk_u_8_n_idx_btree", PkU8{N: n, Data: data})
}

func deletePkU8(_ server.ReducerContext, n uint8) {
	runtime.DeleteBy[PkU8, uint8]("pk_u_8_n_idx_btree", n)
}

// --- PkU16 ---

func insertPkU16(_ server.ReducerContext, n uint16, data int32) {
	runtime.Insert(PkU16{N: n, Data: data})
}

func updatePkU16(_ server.ReducerContext, n uint16, data int32) {
	runtime.UpdateBy[PkU16]("pk_u_16_n_idx_btree", PkU16{N: n, Data: data})
}

func deletePkU16(_ server.ReducerContext, n uint16) {
	runtime.DeleteBy[PkU16, uint16]("pk_u_16_n_idx_btree", n)
}

// --- PkU32 ---

func insertPkU32(_ server.ReducerContext, n uint32, data int32) {
	runtime.Insert(PkU32{N: n, Data: data})
}

func updatePkU32(_ server.ReducerContext, n uint32, data int32) {
	runtime.UpdateBy[PkU32]("pk_u_32_n_idx_btree", PkU32{N: n, Data: data})
}

func deletePkU32(_ server.ReducerContext, n uint32) {
	runtime.DeleteBy[PkU32, uint32]("pk_u_32_n_idx_btree", n)
}

// --- PkU64 ---

func insertPkU64(_ server.ReducerContext, n uint64, data int32) {
	runtime.Insert(PkU64{N: n, Data: data})
}

func updatePkU64(_ server.ReducerContext, n uint64, data int32) {
	runtime.UpdateBy[PkU64]("pk_u_64_n_idx_btree", PkU64{N: n, Data: data})
}

func deletePkU64(_ server.ReducerContext, n uint64) {
	runtime.DeleteBy[PkU64, uint64]("pk_u_64_n_idx_btree", n)
}

// --- PkU128 ---

func insertPkU128(_ server.ReducerContext, n types.Uint128, data int32) {
	runtime.Insert(PkU128{N: n, Data: data})
}

func updatePkU128(_ server.ReducerContext, n types.Uint128, data int32) {
	runtime.UpdateBy[PkU128]("pk_u_128_n_idx_btree", PkU128{N: n, Data: data})
}

func deletePkU128(_ server.ReducerContext, n types.Uint128) {
	runtime.DeleteBy[PkU128, types.Uint128]("pk_u_128_n_idx_btree", n)
}

// --- PkU256 ---

func insertPkU256(_ server.ReducerContext, n types.Uint256, data int32) {
	runtime.Insert(PkU256{N: n, Data: data})
}

func updatePkU256(_ server.ReducerContext, n types.Uint256, data int32) {
	runtime.UpdateBy[PkU256]("pk_u_256_n_idx_btree", PkU256{N: n, Data: data})
}

func deletePkU256(_ server.ReducerContext, n types.Uint256) {
	runtime.DeleteBy[PkU256, types.Uint256]("pk_u_256_n_idx_btree", n)
}

// --- PkI8 ---

func insertPkI8(_ server.ReducerContext, n int8, data int32) {
	runtime.Insert(PkI8{N: n, Data: data})
}

func updatePkI8(_ server.ReducerContext, n int8, data int32) {
	runtime.UpdateBy[PkI8]("pk_i_8_n_idx_btree", PkI8{N: n, Data: data})
}

func deletePkI8(_ server.ReducerContext, n int8) {
	runtime.DeleteBy[PkI8, int8]("pk_i_8_n_idx_btree", n)
}

// --- PkI16 ---

func insertPkI16(_ server.ReducerContext, n int16, data int32) {
	runtime.Insert(PkI16{N: n, Data: data})
}

func updatePkI16(_ server.ReducerContext, n int16, data int32) {
	runtime.UpdateBy[PkI16]("pk_i_16_n_idx_btree", PkI16{N: n, Data: data})
}

func deletePkI16(_ server.ReducerContext, n int16) {
	runtime.DeleteBy[PkI16, int16]("pk_i_16_n_idx_btree", n)
}

// --- PkI32 ---

func insertPkI32(_ server.ReducerContext, n int32, data int32) {
	runtime.Insert(PkI32{N: n, Data: data})
}

func updatePkI32(_ server.ReducerContext, n int32, data int32) {
	runtime.UpdateBy[PkI32]("pk_i_32_n_idx_btree", PkI32{N: n, Data: data})
}

func deletePkI32(_ server.ReducerContext, n int32) {
	runtime.DeleteBy[PkI32, int32]("pk_i_32_n_idx_btree", n)
}

// --- PkI64 ---

func insertPkI64(_ server.ReducerContext, n int64, data int32) {
	runtime.Insert(PkI64{N: n, Data: data})
}

func updatePkI64(_ server.ReducerContext, n int64, data int32) {
	runtime.UpdateBy[PkI64]("pk_i_64_n_idx_btree", PkI64{N: n, Data: data})
}

func deletePkI64(_ server.ReducerContext, n int64) {
	runtime.DeleteBy[PkI64, int64]("pk_i_64_n_idx_btree", n)
}

// --- PkI128 ---

func insertPkI128(_ server.ReducerContext, n types.Int128, data int32) {
	runtime.Insert(PkI128{N: n, Data: data})
}

func updatePkI128(_ server.ReducerContext, n types.Int128, data int32) {
	runtime.UpdateBy[PkI128]("pk_i_128_n_idx_btree", PkI128{N: n, Data: data})
}

func deletePkI128(_ server.ReducerContext, n types.Int128) {
	runtime.DeleteBy[PkI128, types.Int128]("pk_i_128_n_idx_btree", n)
}

// --- PkI256 ---

func insertPkI256(_ server.ReducerContext, n types.Int256, data int32) {
	runtime.Insert(PkI256{N: n, Data: data})
}

func updatePkI256(_ server.ReducerContext, n types.Int256, data int32) {
	runtime.UpdateBy[PkI256]("pk_i_256_n_idx_btree", PkI256{N: n, Data: data})
}

func deletePkI256(_ server.ReducerContext, n types.Int256) {
	runtime.DeleteBy[PkI256, types.Int256]("pk_i_256_n_idx_btree", n)
}

// --- PkBool ---

func insertPkBool(_ server.ReducerContext, b bool, data int32) {
	runtime.Insert(PkBool{B: b, Data: data})
}

func updatePkBool(_ server.ReducerContext, b bool, data int32) {
	runtime.UpdateBy[PkBool]("pk_bool_b_idx_btree", PkBool{B: b, Data: data})
}

func deletePkBool(_ server.ReducerContext, b bool) {
	runtime.DeleteBy[PkBool, bool]("pk_bool_b_idx_btree", b)
}

// --- PkString ---

func insertPkString(_ server.ReducerContext, s string, data int32) {
	runtime.Insert(PkString{S: s, Data: data})
}

func updatePkString(_ server.ReducerContext, s string, data int32) {
	runtime.UpdateBy[PkString]("pk_string_s_idx_btree", PkString{S: s, Data: data})
}

func deletePkString(_ server.ReducerContext, s string) {
	runtime.DeleteBy[PkString, string]("pk_string_s_idx_btree", s)
}

// --- PkIdentity ---

func insertPkIdentity(_ server.ReducerContext, i types.Identity, data int32) {
	runtime.Insert(PkIdentity{I: i, Data: data})
}

func updatePkIdentity(_ server.ReducerContext, i types.Identity, data int32) {
	runtime.UpdateBy[PkIdentity]("pk_identity_i_idx_btree", PkIdentity{I: i, Data: data})
}

func deletePkIdentity(_ server.ReducerContext, i types.Identity) {
	runtime.DeleteBy[PkIdentity, types.Identity]("pk_identity_i_idx_btree", i)
}

// --- PkConnectionId ---

func insertPkConnectionId(_ server.ReducerContext, a types.ConnectionId, data int32) {
	runtime.Insert(PkConnectionId{A: a, Data: data})
}

func updatePkConnectionId(_ server.ReducerContext, a types.ConnectionId, data int32) {
	runtime.UpdateBy[PkConnectionId]("pk_connection_id_a_idx_btree", PkConnectionId{A: a, Data: data})
}

func deletePkConnectionId(_ server.ReducerContext, a types.ConnectionId) {
	runtime.DeleteBy[PkConnectionId, types.ConnectionId]("pk_connection_id_a_idx_btree", a)
}

// --- PkUuid ---

func insertPkUuid(_ server.ReducerContext, u types.Uuid, data int32) {
	runtime.Insert(PkUuid{U: u, Data: data})
}

func updatePkUuid(_ server.ReducerContext, u types.Uuid, data int32) {
	runtime.UpdateBy[PkUuid]("pk_uuid_u_idx_btree", PkUuid{U: u, Data: data})
}

func deletePkUuid(_ server.ReducerContext, u types.Uuid) {
	runtime.DeleteBy[PkUuid, types.Uuid]("pk_uuid_u_idx_btree", u)
}

// --- PkSimpleEnum (insert only) ---

func insertPkSimpleEnum(_ server.ReducerContext, a SimpleEnum, data int32) {
	runtime.Insert(PkSimpleEnum{A: a, Data: data})
}

// --- PkU32Two ---

func insertPkU32Two(_ server.ReducerContext, n uint32, data int32) {
	runtime.Insert(PkU32Two{N: n, Data: data})
}

func updatePkU32Two(_ server.ReducerContext, n uint32, data int32) {
	runtime.UpdateBy[PkU32Two]("pk_u_32_two_n_idx_btree", PkU32Two{N: n, Data: data})
}

func deletePkU32Two(_ server.ReducerContext, n uint32) {
	runtime.DeleteBy[PkU32Two, uint32]("pk_u_32_two_n_idx_btree", n)
}

// ---------------------------------------------------------------------------
// Special reducers
// ---------------------------------------------------------------------------

// updatePkSimpleEnum finds an existing row by pk, then updates it.
func updatePkSimpleEnum(_ server.ReducerContext, a SimpleEnum, data int32) error {
	_, found, err := runtime.FindBy[PkSimpleEnum, SimpleEnum]("pk_simple_enum_a_idx_btree", a)
	if err != nil {
		return err
	}
	if !found {
		return fmt.Errorf("row not found")
	}
	runtime.UpdateBy[PkSimpleEnum]("pk_simple_enum_a_idx_btree", PkSimpleEnum{A: a, Data: data})
	return nil
}

func insertLargeTable(_ server.ReducerContext, a uint8, b uint16, c uint32, d uint64, e types.Uint128, f types.Uint256, g int8, h int16, i int32, j int64, k types.Int128, l types.Int256, m bool, n float32, o float64, p string, q SimpleEnum, r EnumWithPayload, s UnitStruct, t ByteStruct, u EveryPrimitiveStruct, v EveryVecStruct) {
	runtime.Insert(LargeTable{A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q, R: r, S: s, T: t, U: u, V: v})
}

func deleteLargeTable(_ server.ReducerContext, a uint8, b uint16, c uint32, d uint64, e types.Uint128, f types.Uint256, g int8, h int16, i int32, j int64, k types.Int128, l types.Int256, m bool, n float32, o float64, p string, q SimpleEnum, r EnumWithPayload, s UnitStruct, t ByteStruct, u EveryPrimitiveStruct, v EveryVecStruct) {
	runtime.Delete(LargeTable{A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q, R: r, S: s, T: t, U: u, V: v})
}

// insertTableHoldsTable inserts a row that holds instances of other table structs.
func insertTableHoldsTable(_ server.ReducerContext, a OneU8, b VecU8) {
	runtime.Insert(TableHoldsTable{A: a, B: b})
}

// insertIntoBtreeU32 batch-inserts rows into the BTreeU32 table.
func insertIntoBtreeU32(_ server.ReducerContext, rows []BTreeU32) {
	for _, row := range rows {
		runtime.Insert(row)
	}
}

// deleteFromBtreeU32 batch-deletes rows from the BTreeU32 table.
func deleteFromBtreeU32(_ server.ReducerContext, rows []BTreeU32) {
	for _, row := range rows {
		runtime.Delete(row)
	}
}

// insertIntoPkBtreeU32 inserts into both pk_u32 and btree_u32 tables.
func insertIntoPkBtreeU32(_ server.ReducerContext, pkU32 []PkU32, btU32 []BTreeU32) {
	for _, row := range pkU32 {
		runtime.Insert(row)
	}
	for _, row := range btU32 {
		runtime.Insert(row)
	}
}

// insertUniqueU32UpdatePkU32 inserts a UniqueU32 row and updates a PkU32 row.
func insertUniqueU32UpdatePkU32(_ server.ReducerContext, n uint32, dUnique int32, dPk int32) {
	runtime.Insert(UniqueU32{N: n, Data: dUnique})
	runtime.UpdateBy[PkU32]("pk_u_32_n_idx_btree", PkU32{N: n, Data: dPk})
}

// deletePkU32InsertPkU32Two inserts a PkU32Two row and deletes a PkU32 row.
func deletePkU32InsertPkU32Two(_ server.ReducerContext, n uint32, data int32) {
	runtime.Insert(PkU32Two{N: n, Data: data})
	runtime.Delete(PkU32{N: n, Data: data})
}

// ---------------------------------------------------------------------------
// Caller identity/connection reducers -- use ctx.Sender() and ctx.ConnectionId().
// ---------------------------------------------------------------------------

func insertCallerOneIdentity(ctx server.ReducerContext) {
	runtime.Insert(OneIdentity{I: ctx.Sender()})
}

func insertCallerVecIdentity(ctx server.ReducerContext) {
	runtime.Insert(VecIdentity{I: []types.Identity{ctx.Sender()}})
}

func insertCallerUniqueIdentity(ctx server.ReducerContext, data int32) {
	runtime.Insert(UniqueIdentity{I: ctx.Sender(), Data: data})
}

func insertCallerPkIdentity(ctx server.ReducerContext, data int32) {
	runtime.Insert(PkIdentity{I: ctx.Sender(), Data: data})
}

func insertCallerOneConnectionId(ctx server.ReducerContext) {
	runtime.Insert(OneConnectionId{A: ctx.ConnectionId()})
}

func insertCallerVecConnectionId(ctx server.ReducerContext) {
	runtime.Insert(VecConnectionId{A: []types.ConnectionId{ctx.ConnectionId()}})
}

func insertCallerUniqueConnectionId(ctx server.ReducerContext, data int32) {
	runtime.Insert(UniqueConnectionId{A: ctx.ConnectionId(), Data: data})
}

func insertCallerPkConnectionId(ctx server.ReducerContext, data int32) {
	runtime.Insert(PkConnectionId{A: ctx.ConnectionId(), Data: data})
}

// ---------------------------------------------------------------------------
// Timestamp and UUID reducers
// ---------------------------------------------------------------------------

func insertCallTimestamp(ctx server.ReducerContext) {
	runtime.Insert(OneTimestamp{T: ctx.Timestamp()})
}

// insertCallUuidV4 generates a v4 UUID from the reducer context timestamp
// and inserts it into the one_uuid table.
// NOTE: The Go SDK ReducerContext does not yet expose NewUuidV4(). As a
// workaround we derive a deterministic UUID from the timestamp bytes.
func insertCallUuidV4(ctx server.ReducerContext) {
	ts := ctx.Timestamp()
	var b [16]byte
	// Use timestamp microseconds to seed the UUID bytes.
	usec := ts.Microseconds()
	b[0] = byte(usec)
	b[1] = byte(usec >> 8)
	b[2] = byte(usec >> 16)
	b[3] = byte(usec >> 24)
	b[4] = byte(usec >> 32)
	b[5] = byte(usec >> 40)
	b[6] = byte(usec >> 48)
	b[7] = byte(usec >> 56)
	// Set version 4 bits
	b[6] = (b[6] & 0x0f) | 0x40
	// Set variant bits
	b[8] = (b[8] & 0x3f) | 0x80
	runtime.Insert(OneUuid{U: types.NewUuid(b)})
}

// insertCallUuidV7 generates a v7 UUID from the reducer context timestamp
// and inserts it into the one_uuid table.
// NOTE: The Go SDK ReducerContext does not yet expose NewUuidV7(). As a
// workaround we derive a deterministic UUID from the timestamp bytes.
func insertCallUuidV7(ctx server.ReducerContext) {
	ts := ctx.Timestamp()
	var b [16]byte
	// Use timestamp microseconds as the time component.
	usec := ts.Microseconds()
	msec := usec / 1000
	b[0] = byte(msec >> 40)
	b[1] = byte(msec >> 32)
	b[2] = byte(msec >> 24)
	b[3] = byte(msec >> 16)
	b[4] = byte(msec >> 8)
	b[5] = byte(msec)
	// Set version 7 bits
	b[6] = (b[6] & 0x0f) | 0x70
	// Set variant bits
	b[8] = (b[8] & 0x3f) | 0x80
	runtime.Insert(OneUuid{U: types.NewUuid(b)})
}

// ---------------------------------------------------------------------------
// insertPrimitivesAsStrings converts each field of EveryPrimitiveStruct to a
// string and inserts the result into VecString.
// ---------------------------------------------------------------------------

func insertPrimitivesAsStrings(_ server.ReducerContext, s EveryPrimitiveStruct) {
	runtime.Insert(VecString{S: []string{
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

func noOpSucceeds(_ server.ReducerContext) {}

func sendScheduledMessage(_ server.ReducerContext, arg ScheduledTable) {
	// No-op: the test just checks the reducer exists.
	_ = arg.Text
	_ = arg.ScheduledAt
	_ = arg.ScheduledId
}

func insertUser(_ server.ReducerContext, name string, identity types.Identity) {
	runtime.Insert(Users{Identity: identity, Name: name})
}

func insertIntoIndexedSimpleEnum(_ server.ReducerContext, n SimpleEnum) {
	runtime.Insert(IndexedSimpleEnum{N: n})
}

func updateIndexedSimpleEnum(_ server.ReducerContext, a SimpleEnum, b SimpleEnum) error {
	iter, err := runtime.Scan[IndexedSimpleEnum]()
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
			runtime.Delete(row)
			found = true
			break
		}
	}
	if found {
		runtime.Insert(IndexedSimpleEnum{N: b})
	}
	return nil
}

// sortedUuidsInsert generates 1000 v7 UUIDs from the context timestamp and
// inserts them into pk_uuid, then verifies they are sorted.
// NOTE: The Go SDK ReducerContext does not yet expose NewUuidV7(). As a
// workaround we derive deterministic UUIDs from the timestamp + counter.
func sortedUuidsInsert(ctx server.ReducerContext) error {
	ts := ctx.Timestamp()
	usec := ts.Microseconds()
	msec := usec / 1000

	for i := 0; i < 1000; i++ {
		var b [16]byte
		// Encode milliseconds in big-endian (bytes 0-5)
		b[0] = byte(msec >> 40)
		b[1] = byte(msec >> 32)
		b[2] = byte(msec >> 24)
		b[3] = byte(msec >> 16)
		b[4] = byte(msec >> 8)
		b[5] = byte(msec)
		// Use counter in sub-millisecond portion for ordering
		counter := uint16(i)
		b[6] = byte(counter>>8) | 0x70 // version 7
		b[7] = byte(counter)
		b[8] = 0x80 // variant
		b[9] = byte(i)

		uuid := types.NewUuid(b)
		runtime.Insert(PkUuid{U: uuid, Data: 0})
	}

	// Verify UUIDs are sorted.
	iter, err := runtime.Scan[PkUuid]()
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
