package main

// This file consolidates all registrations into a single init() function
// to ensure correct ordering: sum types → tables → reducers.
// Go processes init() functions in lexical file order within a package,
// so this file (init.go) runs before reducers.go and tables.go.

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
)

func init() {
	// -----------------------------------------------------------------
	// 0. Simple enum registrations (must come BEFORE sum types that reference them)
	// -----------------------------------------------------------------
	server.RegisterSimpleEnum[SimpleEnum]("Zero", "One", "Two")

	// -----------------------------------------------------------------
	// 1. Sum type registrations (must come BEFORE table/reducer registrations)
	// -----------------------------------------------------------------

	// Register EnumWithPayload sum type
	server.RegisterSumType[EnumWithPayload](
		server.Variant[EnumWithPayloadU8]("U8"),
		server.Variant[EnumWithPayloadU16]("U16"),
		server.Variant[EnumWithPayloadU32]("U32"),
		server.Variant[EnumWithPayloadU64]("U64"),
		server.Variant[EnumWithPayloadU128]("U128"),
		server.Variant[EnumWithPayloadU256]("U256"),
		server.Variant[EnumWithPayloadI8]("I8"),
		server.Variant[EnumWithPayloadI16]("I16"),
		server.Variant[EnumWithPayloadI32]("I32"),
		server.Variant[EnumWithPayloadI64]("I64"),
		server.Variant[EnumWithPayloadI128]("I128"),
		server.Variant[EnumWithPayloadI256]("I256"),
		server.Variant[EnumWithPayloadBool]("Bool"),
		server.Variant[EnumWithPayloadF32]("F32"),
		server.Variant[EnumWithPayloadF64]("F64"),
		server.Variant[EnumWithPayloadStr]("Str"),
		server.Variant[EnumWithPayloadIdentity]("Identity"),
		server.Variant[EnumWithPayloadConnectionId]("ConnectionId"),
		server.Variant[EnumWithPayloadTimestamp]("Timestamp"),
		server.Variant[EnumWithPayloadUuid]("Uuid"),
		server.Variant[EnumWithPayloadBytes]("Bytes"),
		server.Variant[EnumWithPayloadInts]("Ints"),
		server.Variant[EnumWithPayloadStrings]("Strings"),
		server.Variant[EnumWithPayloadSimpleEnums]("SimpleEnums"),
	)

	// Register ResultI32StringValue sum type
	server.RegisterSumType[ResultI32StringValue](
		server.Variant[ResultI32StringOk]("ok"),
		server.Variant[ResultI32StringErr]("err"),
	)

	// Register ResultStringI32Value sum type
	server.RegisterSumType[ResultStringI32Value](
		server.Variant[ResultStringI32Ok]("ok"),
		server.Variant[ResultStringI32Err]("err"),
	)

	// Register ResultIdentityStringValue sum type
	server.RegisterSumType[ResultIdentityStringValue](
		server.Variant[ResultIdentityStringOk]("ok"),
		server.Variant[ResultIdentityStringErr]("err"),
	)

	// Register ResultSimpleEnumI32Value sum type
	server.RegisterSumType[ResultSimpleEnumI32Value](
		server.Variant[ResultSimpleEnumI32Ok]("ok"),
		server.Variant[ResultSimpleEnumI32Err]("err"),
	)

	// Register ResultEveryPrimitiveStructStringValue sum type
	server.RegisterSumType[ResultEveryPrimitiveStructStringValue](
		server.Variant[ResultEveryPrimitiveStructStringOk]("ok"),
		server.Variant[ResultEveryPrimitiveStructStringErr]("err"),
	)

	// Register ResultVecI32StringValue sum type
	server.RegisterSumType[ResultVecI32StringValue](
		server.Variant[ResultVecI32StringOk]("ok"),
		server.Variant[ResultVecI32StringErr]("err"),
	)

	// -----------------------------------------------------------------
	// 2. Table registrations
	// -----------------------------------------------------------------

	// One* tables (26 total, public)
	server.RegisterTable[OneU8]("one_u_8", server.TableAccessPublic)
	server.RegisterTable[OneU16]("one_u_16", server.TableAccessPublic)
	server.RegisterTable[OneU32]("one_u_32", server.TableAccessPublic)
	server.RegisterTable[OneU64]("one_u_64", server.TableAccessPublic)
	server.RegisterTable[OneU128]("one_u_128", server.TableAccessPublic)
	server.RegisterTable[OneU256]("one_u_256", server.TableAccessPublic)
	server.RegisterTable[OneI8]("one_i_8", server.TableAccessPublic)
	server.RegisterTable[OneI16]("one_i_16", server.TableAccessPublic)
	server.RegisterTable[OneI32]("one_i_32", server.TableAccessPublic)
	server.RegisterTable[OneI64]("one_i_64", server.TableAccessPublic)
	server.RegisterTable[OneI128]("one_i_128", server.TableAccessPublic)
	server.RegisterTable[OneI256]("one_i_256", server.TableAccessPublic)
	server.RegisterTable[OneBool]("one_bool", server.TableAccessPublic)
	server.RegisterTable[OneF32]("one_f_32", server.TableAccessPublic)
	server.RegisterTable[OneF64]("one_f_64", server.TableAccessPublic)
	server.RegisterTable[OneString]("one_string", server.TableAccessPublic)
	server.RegisterTable[OneIdentity]("one_identity", server.TableAccessPublic)
	server.RegisterTable[OneConnectionId]("one_connection_id", server.TableAccessPublic)
	server.RegisterTable[OneUuid]("one_uuid", server.TableAccessPublic)
	server.RegisterTable[OneTimestamp]("one_timestamp", server.TableAccessPublic)
	server.RegisterTable[OneSimpleEnum]("one_simple_enum", server.TableAccessPublic)
	server.RegisterTable[OneEnumWithPayload]("one_enum_with_payload", server.TableAccessPublic)
	server.RegisterTable[OneUnitStruct]("one_unit_struct", server.TableAccessPublic)
	server.RegisterTable[OneByteStruct]("one_byte_struct", server.TableAccessPublic)
	server.RegisterTable[OneEveryPrimitiveStruct]("one_every_primitive_struct", server.TableAccessPublic)
	server.RegisterTable[OneEveryVecStruct]("one_every_vec_struct", server.TableAccessPublic)

	// Vec* tables (26 total, public)
	server.RegisterTable[VecU8]("vec_u_8", server.TableAccessPublic)
	server.RegisterTable[VecU16]("vec_u_16", server.TableAccessPublic)
	server.RegisterTable[VecU32]("vec_u_32", server.TableAccessPublic)
	server.RegisterTable[VecU64]("vec_u_64", server.TableAccessPublic)
	server.RegisterTable[VecU128]("vec_u_128", server.TableAccessPublic)
	server.RegisterTable[VecU256]("vec_u_256", server.TableAccessPublic)
	server.RegisterTable[VecI8]("vec_i_8", server.TableAccessPublic)
	server.RegisterTable[VecI16]("vec_i_16", server.TableAccessPublic)
	server.RegisterTable[VecI32]("vec_i_32", server.TableAccessPublic)
	server.RegisterTable[VecI64]("vec_i_64", server.TableAccessPublic)
	server.RegisterTable[VecI128]("vec_i_128", server.TableAccessPublic)
	server.RegisterTable[VecI256]("vec_i_256", server.TableAccessPublic)
	server.RegisterTable[VecBool]("vec_bool", server.TableAccessPublic)
	server.RegisterTable[VecF32]("vec_f_32", server.TableAccessPublic)
	server.RegisterTable[VecF64]("vec_f_64", server.TableAccessPublic)
	server.RegisterTable[VecString]("vec_string", server.TableAccessPublic)
	server.RegisterTable[VecIdentity]("vec_identity", server.TableAccessPublic)
	server.RegisterTable[VecConnectionId]("vec_connection_id", server.TableAccessPublic)
	server.RegisterTable[VecUuid]("vec_uuid", server.TableAccessPublic)
	server.RegisterTable[VecTimestamp]("vec_timestamp", server.TableAccessPublic)
	server.RegisterTable[VecSimpleEnum]("vec_simple_enum", server.TableAccessPublic)
	server.RegisterTable[VecEnumWithPayload]("vec_enum_with_payload", server.TableAccessPublic)
	server.RegisterTable[VecUnitStruct]("vec_unit_struct", server.TableAccessPublic)
	server.RegisterTable[VecByteStruct]("vec_byte_struct", server.TableAccessPublic)
	server.RegisterTable[VecEveryPrimitiveStruct]("vec_every_primitive_struct", server.TableAccessPublic)
	server.RegisterTable[VecEveryVecStruct]("vec_every_vec_struct", server.TableAccessPublic)

	// Option* tables (7 total, public)
	server.RegisterTable[OptionI32]("option_i_32", server.TableAccessPublic)
	server.RegisterTable[OptionString]("option_string", server.TableAccessPublic)
	server.RegisterTable[OptionIdentity]("option_identity", server.TableAccessPublic)
	server.RegisterTable[OptionUuid]("option_uuid", server.TableAccessPublic)
	server.RegisterTable[OptionSimpleEnum]("option_simple_enum", server.TableAccessPublic)
	server.RegisterTable[OptionEveryPrimitiveStruct]("option_every_primitive_struct", server.TableAccessPublic)
	server.RegisterTable[OptionVecOptionI32]("option_vec_option_i_32", server.TableAccessPublic)

	// Result* tables (6 total, public)
	server.RegisterTable[ResultI32String]("result_i_32_string", server.TableAccessPublic)
	server.RegisterTable[ResultStringI32]("result_string_i_32", server.TableAccessPublic)
	server.RegisterTable[ResultIdentityString]("result_identity_string", server.TableAccessPublic)
	server.RegisterTable[ResultSimpleEnumI32]("result_simple_enum_i_32", server.TableAccessPublic)
	server.RegisterTable[ResultEveryPrimitiveStructString]("result_every_primitive_struct_string", server.TableAccessPublic)
	server.RegisterTable[ResultVecI32String]("result_vec_i_32_string", server.TableAccessPublic)

	// Unique* tables (17 total, public)
	server.RegisterTable[UniqueU8]("unique_u_8", server.TableAccessPublic)
	server.RegisterTable[UniqueU16]("unique_u_16", server.TableAccessPublic)
	server.RegisterTable[UniqueU32]("unique_u_32", server.TableAccessPublic)
	server.RegisterTable[UniqueU64]("unique_u_64", server.TableAccessPublic)
	server.RegisterTable[UniqueU128]("unique_u_128", server.TableAccessPublic)
	server.RegisterTable[UniqueU256]("unique_u_256", server.TableAccessPublic)
	server.RegisterTable[UniqueI8]("unique_i_8", server.TableAccessPublic)
	server.RegisterTable[UniqueI16]("unique_i_16", server.TableAccessPublic)
	server.RegisterTable[UniqueI32]("unique_i_32", server.TableAccessPublic)
	server.RegisterTable[UniqueI64]("unique_i_64", server.TableAccessPublic)
	server.RegisterTable[UniqueI128]("unique_i_128", server.TableAccessPublic)
	server.RegisterTable[UniqueI256]("unique_i_256", server.TableAccessPublic)
	server.RegisterTable[UniqueBool]("unique_bool", server.TableAccessPublic)
	server.RegisterTable[UniqueString]("unique_string", server.TableAccessPublic)
	server.RegisterTable[UniqueIdentity]("unique_identity", server.TableAccessPublic)
	server.RegisterTable[UniqueConnectionId]("unique_connection_id", server.TableAccessPublic)
	server.RegisterTable[UniqueUuid]("unique_uuid", server.TableAccessPublic)

	// Pk* tables (19 total, public)
	server.RegisterTable[PkU8]("pk_u_8", server.TableAccessPublic)
	server.RegisterTable[PkU16]("pk_u_16", server.TableAccessPublic)
	server.RegisterTable[PkU32]("pk_u_32", server.TableAccessPublic)
	server.RegisterTable[PkU32Two]("pk_u_32_two", server.TableAccessPublic)
	server.RegisterTable[PkU64]("pk_u_64", server.TableAccessPublic)
	server.RegisterTable[PkU128]("pk_u_128", server.TableAccessPublic)
	server.RegisterTable[PkU256]("pk_u_256", server.TableAccessPublic)
	server.RegisterTable[PkI8]("pk_i_8", server.TableAccessPublic)
	server.RegisterTable[PkI16]("pk_i_16", server.TableAccessPublic)
	server.RegisterTable[PkI32]("pk_i_32", server.TableAccessPublic)
	server.RegisterTable[PkI64]("pk_i_64", server.TableAccessPublic)
	server.RegisterTable[PkI128]("pk_i_128", server.TableAccessPublic)
	server.RegisterTable[PkI256]("pk_i_256", server.TableAccessPublic)
	server.RegisterTable[PkBool]("pk_bool", server.TableAccessPublic)
	server.RegisterTable[PkString]("pk_string", server.TableAccessPublic)
	server.RegisterTable[PkIdentity]("pk_identity", server.TableAccessPublic)
	server.RegisterTable[PkConnectionId]("pk_connection_id", server.TableAccessPublic)
	server.RegisterTable[PkUuid]("pk_uuid", server.TableAccessPublic)
	server.RegisterTable[PkSimpleEnum]("pk_simple_enum", server.TableAccessPublic)

	// Special tables
	server.RegisterTable[BTreeU32]("btree_u32", server.TableAccessPublic)
	server.RegisterTable[Users]("users", server.TableAccessPublic)
	server.RegisterTable[IndexedTable]("indexed_table", server.TableAccessPrivate)
	server.RegisterTable[IndexedTable2]("indexed_table_2", server.TableAccessPrivate)
	server.RegisterTable[IndexedSimpleEnum]("indexed_simple_enum", server.TableAccessPublic)
	server.RegisterTable[LargeTable]("large_table", server.TableAccessPublic)
	server.RegisterTable[TableHoldsTable]("table_holds_table", server.TableAccessPublic)
	server.RegisterTable[ScheduledTable]("scheduled_table", server.TableAccessPublic)

	// -----------------------------------------------------------------
	// 3. Reducer registrations
	// Parameter names must match the Rust sdk-test module definitions.
	// -----------------------------------------------------------------

	// One* inserts (26 total) — single param named after the table field
	server.RegisterReducer("insert_one_u8", insertOneU8, "n")
	server.RegisterReducer("insert_one_u16", insertOneU16, "n")
	server.RegisterReducer("insert_one_u32", insertOneU32, "n")
	server.RegisterReducer("insert_one_u64", insertOneU64, "n")
	server.RegisterReducer("insert_one_u128", insertOneU128, "n")
	server.RegisterReducer("insert_one_u256", insertOneU256, "n")
	server.RegisterReducer("insert_one_i8", insertOneI8, "n")
	server.RegisterReducer("insert_one_i16", insertOneI16, "n")
	server.RegisterReducer("insert_one_i32", insertOneI32, "n")
	server.RegisterReducer("insert_one_i64", insertOneI64, "n")
	server.RegisterReducer("insert_one_i128", insertOneI128, "n")
	server.RegisterReducer("insert_one_i256", insertOneI256, "n")
	server.RegisterReducer("insert_one_bool", insertOneBool, "b")
	server.RegisterReducer("insert_one_f32", insertOneF32, "f")
	server.RegisterReducer("insert_one_f64", insertOneF64, "f")
	server.RegisterReducer("insert_one_string", insertOneString, "s")
	server.RegisterReducer("insert_one_identity", insertOneIdentity, "i")
	server.RegisterReducer("insert_one_connection_id", insertOneConnectionId, "a")
	server.RegisterReducer("insert_one_uuid", insertOneUuid, "u")
	server.RegisterReducer("insert_one_timestamp", insertOneTimestamp, "t")
	server.RegisterReducer("insert_one_simple_enum", insertOneSimpleEnum, "e")
	server.RegisterReducer("insert_one_enum_with_payload", insertOneEnumWithPayload, "e")
	server.RegisterReducer("insert_one_unit_struct", insertOneUnitStruct, "s")
	server.RegisterReducer("insert_one_byte_struct", insertOneByteStruct, "s")
	server.RegisterReducer("insert_one_every_primitive_struct", insertOneEveryPrimitiveStruct, "s")
	server.RegisterReducer("insert_one_every_vec_struct", insertOneEveryVecStruct, "s")

	// Vec* inserts (26 total)
	server.RegisterReducer("insert_vec_u8", insertVecU8, "n")
	server.RegisterReducer("insert_vec_u16", insertVecU16, "n")
	server.RegisterReducer("insert_vec_u32", insertVecU32, "n")
	server.RegisterReducer("insert_vec_u64", insertVecU64, "n")
	server.RegisterReducer("insert_vec_u128", insertVecU128, "n")
	server.RegisterReducer("insert_vec_u256", insertVecU256, "n")
	server.RegisterReducer("insert_vec_i8", insertVecI8, "n")
	server.RegisterReducer("insert_vec_i16", insertVecI16, "n")
	server.RegisterReducer("insert_vec_i32", insertVecI32, "n")
	server.RegisterReducer("insert_vec_i64", insertVecI64, "n")
	server.RegisterReducer("insert_vec_i128", insertVecI128, "n")
	server.RegisterReducer("insert_vec_i256", insertVecI256, "n")
	server.RegisterReducer("insert_vec_bool", insertVecBool, "b")
	server.RegisterReducer("insert_vec_f32", insertVecF32, "f")
	server.RegisterReducer("insert_vec_f64", insertVecF64, "f")
	server.RegisterReducer("insert_vec_string", insertVecString, "s")
	server.RegisterReducer("insert_vec_identity", insertVecIdentity, "i")
	server.RegisterReducer("insert_vec_connection_id", insertVecConnectionId, "a")
	server.RegisterReducer("insert_vec_uuid", insertVecUuid, "u")
	server.RegisterReducer("insert_vec_timestamp", insertVecTimestamp, "t")
	server.RegisterReducer("insert_vec_simple_enum", insertVecSimpleEnum, "e")
	server.RegisterReducer("insert_vec_enum_with_payload", insertVecEnumWithPayload, "e")
	server.RegisterReducer("insert_vec_unit_struct", insertVecUnitStruct, "s")
	server.RegisterReducer("insert_vec_byte_struct", insertVecByteStruct, "s")
	server.RegisterReducer("insert_vec_every_primitive_struct", insertVecEveryPrimitiveStruct, "s")
	server.RegisterReducer("insert_vec_every_vec_struct", insertVecEveryVecStruct, "s")

	// Option* inserts (7 total)
	server.RegisterReducer("insert_option_i32", insertOptionI32, "n")
	server.RegisterReducer("insert_option_string", insertOptionString, "s")
	server.RegisterReducer("insert_option_identity", insertOptionIdentity, "i")
	server.RegisterReducer("insert_option_uuid", insertOptionUuid, "u")
	server.RegisterReducer("insert_option_simple_enum", insertOptionSimpleEnum, "e")
	server.RegisterReducer("insert_option_every_primitive_struct", insertOptionEveryPrimitiveStruct, "s")
	server.RegisterReducer("insert_option_vec_option_i32", insertOptionVecOptionI32, "v")

	// Result* inserts (6 total)
	server.RegisterReducer("insert_result_i32_string", insertResultI32String, "r")
	server.RegisterReducer("insert_result_string_i32", insertResultStringI32, "r")
	server.RegisterReducer("insert_result_identity_string", insertResultIdentityString, "r")
	server.RegisterReducer("insert_result_simple_enum_i32", insertResultSimpleEnumI32, "r")
	server.RegisterReducer("insert_result_every_primitive_struct_string", insertResultEveryPrimitiveStructString, "r")
	server.RegisterReducer("insert_result_vec_i32_string", insertResultVecI32String, "r")

	// Unique* CRUD (17 types x 3 = 51): insert/update have (key, data), delete has (key)
	server.RegisterReducer("insert_unique_u8", insertUniqueU8, "n", "data")
	server.RegisterReducer("update_unique_u8", updateUniqueU8, "n", "data")
	server.RegisterReducer("delete_unique_u8", deleteUniqueU8, "n")
	server.RegisterReducer("insert_unique_u16", insertUniqueU16, "n", "data")
	server.RegisterReducer("update_unique_u16", updateUniqueU16, "n", "data")
	server.RegisterReducer("delete_unique_u16", deleteUniqueU16, "n")
	server.RegisterReducer("insert_unique_u32", insertUniqueU32, "n", "data")
	server.RegisterReducer("update_unique_u32", updateUniqueU32, "n", "data")
	server.RegisterReducer("delete_unique_u32", deleteUniqueU32, "n")
	server.RegisterReducer("insert_unique_u64", insertUniqueU64, "n", "data")
	server.RegisterReducer("update_unique_u64", updateUniqueU64, "n", "data")
	server.RegisterReducer("delete_unique_u64", deleteUniqueU64, "n")
	server.RegisterReducer("insert_unique_u128", insertUniqueU128, "n", "data")
	server.RegisterReducer("update_unique_u128", updateUniqueU128, "n", "data")
	server.RegisterReducer("delete_unique_u128", deleteUniqueU128, "n")
	server.RegisterReducer("insert_unique_u256", insertUniqueU256, "n", "data")
	server.RegisterReducer("update_unique_u256", updateUniqueU256, "n", "data")
	server.RegisterReducer("delete_unique_u256", deleteUniqueU256, "n")
	server.RegisterReducer("insert_unique_i8", insertUniqueI8, "n", "data")
	server.RegisterReducer("update_unique_i8", updateUniqueI8, "n", "data")
	server.RegisterReducer("delete_unique_i8", deleteUniqueI8, "n")
	server.RegisterReducer("insert_unique_i16", insertUniqueI16, "n", "data")
	server.RegisterReducer("update_unique_i16", updateUniqueI16, "n", "data")
	server.RegisterReducer("delete_unique_i16", deleteUniqueI16, "n")
	server.RegisterReducer("insert_unique_i32", insertUniqueI32, "n", "data")
	server.RegisterReducer("update_unique_i32", updateUniqueI32, "n", "data")
	server.RegisterReducer("delete_unique_i32", deleteUniqueI32, "n")
	server.RegisterReducer("insert_unique_i64", insertUniqueI64, "n", "data")
	server.RegisterReducer("update_unique_i64", updateUniqueI64, "n", "data")
	server.RegisterReducer("delete_unique_i64", deleteUniqueI64, "n")
	server.RegisterReducer("insert_unique_i128", insertUniqueI128, "n", "data")
	server.RegisterReducer("update_unique_i128", updateUniqueI128, "n", "data")
	server.RegisterReducer("delete_unique_i128", deleteUniqueI128, "n")
	server.RegisterReducer("insert_unique_i256", insertUniqueI256, "n", "data")
	server.RegisterReducer("update_unique_i256", updateUniqueI256, "n", "data")
	server.RegisterReducer("delete_unique_i256", deleteUniqueI256, "n")
	server.RegisterReducer("insert_unique_bool", insertUniqueBool, "b", "data")
	server.RegisterReducer("update_unique_bool", updateUniqueBool, "b", "data")
	server.RegisterReducer("delete_unique_bool", deleteUniqueBool, "b")
	server.RegisterReducer("insert_unique_string", insertUniqueString, "s", "data")
	server.RegisterReducer("update_unique_string", updateUniqueString, "s", "data")
	server.RegisterReducer("delete_unique_string", deleteUniqueString, "s")
	server.RegisterReducer("insert_unique_identity", insertUniqueIdentity, "i", "data")
	server.RegisterReducer("update_unique_identity", updateUniqueIdentity, "i", "data")
	server.RegisterReducer("delete_unique_identity", deleteUniqueIdentity, "i")
	server.RegisterReducer("insert_unique_connection_id", insertUniqueConnectionId, "a", "data")
	server.RegisterReducer("update_unique_connection_id", updateUniqueConnectionId, "a", "data")
	server.RegisterReducer("delete_unique_connection_id", deleteUniqueConnectionId, "a")
	server.RegisterReducer("insert_unique_uuid", insertUniqueUuid, "u", "data")
	server.RegisterReducer("update_unique_uuid", updateUniqueUuid, "u", "data")
	server.RegisterReducer("delete_unique_uuid", deleteUniqueUuid, "u")

	// Pk* CRUD: insert/update have (key, data), delete has (key)
	server.RegisterReducer("insert_pk_u8", insertPkU8, "n", "data")
	server.RegisterReducer("update_pk_u8", updatePkU8, "n", "data")
	server.RegisterReducer("delete_pk_u8", deletePkU8, "n")
	server.RegisterReducer("insert_pk_u16", insertPkU16, "n", "data")
	server.RegisterReducer("update_pk_u16", updatePkU16, "n", "data")
	server.RegisterReducer("delete_pk_u16", deletePkU16, "n")
	server.RegisterReducer("insert_pk_u32", insertPkU32, "n", "data")
	server.RegisterReducer("update_pk_u32", updatePkU32, "n", "data")
	server.RegisterReducer("delete_pk_u32", deletePkU32, "n")
	server.RegisterReducer("insert_pk_u64", insertPkU64, "n", "data")
	server.RegisterReducer("update_pk_u64", updatePkU64, "n", "data")
	server.RegisterReducer("delete_pk_u64", deletePkU64, "n")
	server.RegisterReducer("insert_pk_u128", insertPkU128, "n", "data")
	server.RegisterReducer("update_pk_u128", updatePkU128, "n", "data")
	server.RegisterReducer("delete_pk_u128", deletePkU128, "n")
	server.RegisterReducer("insert_pk_u256", insertPkU256, "n", "data")
	server.RegisterReducer("update_pk_u256", updatePkU256, "n", "data")
	server.RegisterReducer("delete_pk_u256", deletePkU256, "n")
	server.RegisterReducer("insert_pk_i8", insertPkI8, "n", "data")
	server.RegisterReducer("update_pk_i8", updatePkI8, "n", "data")
	server.RegisterReducer("delete_pk_i8", deletePkI8, "n")
	server.RegisterReducer("insert_pk_i16", insertPkI16, "n", "data")
	server.RegisterReducer("update_pk_i16", updatePkI16, "n", "data")
	server.RegisterReducer("delete_pk_i16", deletePkI16, "n")
	server.RegisterReducer("insert_pk_i32", insertPkI32, "n", "data")
	server.RegisterReducer("update_pk_i32", updatePkI32, "n", "data")
	server.RegisterReducer("delete_pk_i32", deletePkI32, "n")
	server.RegisterReducer("insert_pk_i64", insertPkI64, "n", "data")
	server.RegisterReducer("update_pk_i64", updatePkI64, "n", "data")
	server.RegisterReducer("delete_pk_i64", deletePkI64, "n")
	server.RegisterReducer("insert_pk_i128", insertPkI128, "n", "data")
	server.RegisterReducer("update_pk_i128", updatePkI128, "n", "data")
	server.RegisterReducer("delete_pk_i128", deletePkI128, "n")
	server.RegisterReducer("insert_pk_i256", insertPkI256, "n", "data")
	server.RegisterReducer("update_pk_i256", updatePkI256, "n", "data")
	server.RegisterReducer("delete_pk_i256", deletePkI256, "n")
	server.RegisterReducer("insert_pk_bool", insertPkBool, "b", "data")
	server.RegisterReducer("update_pk_bool", updatePkBool, "b", "data")
	server.RegisterReducer("delete_pk_bool", deletePkBool, "b")
	server.RegisterReducer("insert_pk_string", insertPkString, "s", "data")
	server.RegisterReducer("update_pk_string", updatePkString, "s", "data")
	server.RegisterReducer("delete_pk_string", deletePkString, "s")
	server.RegisterReducer("insert_pk_identity", insertPkIdentity, "i", "data")
	server.RegisterReducer("update_pk_identity", updatePkIdentity, "i", "data")
	server.RegisterReducer("delete_pk_identity", deletePkIdentity, "i")
	server.RegisterReducer("insert_pk_connection_id", insertPkConnectionId, "a", "data")
	server.RegisterReducer("update_pk_connection_id", updatePkConnectionId, "a", "data")
	server.RegisterReducer("delete_pk_connection_id", deletePkConnectionId, "a")
	server.RegisterReducer("insert_pk_uuid", insertPkUuid, "u", "data")
	server.RegisterReducer("update_pk_uuid", updatePkUuid, "u", "data")
	server.RegisterReducer("delete_pk_uuid", deletePkUuid, "u")
	server.RegisterReducer("insert_pk_simple_enum", insertPkSimpleEnum, "a", "data")
	server.RegisterReducer("insert_pk_u32_two", insertPkU32Two, "n", "data")
	server.RegisterReducer("update_pk_u32_two", updatePkU32Two, "n", "data")
	server.RegisterReducer("delete_pk_u32_two", deletePkU32Two, "n")

	// Special reducers
	server.RegisterReducer("update_pk_simple_enum", updatePkSimpleEnum, "a", "data")
	server.RegisterReducer("insert_large_table", insertLargeTable, "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v")
	server.RegisterReducer("delete_large_table", deleteLargeTable, "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v")
	server.RegisterReducer("insert_table_holds_table", insertTableHoldsTable, "a", "b")
	server.RegisterReducer("insert_into_btree_u32", insertIntoBtreeU32, "rows")
	server.RegisterReducer("delete_from_btree_u32", deleteFromBtreeU32, "rows")
	server.RegisterReducer("insert_into_pk_btree_u32", insertIntoPkBtreeU32, "pk_u32", "bt_u32")
	server.RegisterReducer("insert_unique_u32_update_pk_u32", insertUniqueU32UpdatePkU32, "n", "d_unique", "d_pk")
	server.RegisterReducer("delete_pk_u32_insert_pk_u32_two", deletePkU32InsertPkU32Two, "n", "data")

	// Caller identity/connection reducers (no params or single "data" param)
	server.RegisterReducer("insert_caller_one_identity", insertCallerOneIdentity)
	server.RegisterReducer("insert_caller_vec_identity", insertCallerVecIdentity)
	server.RegisterReducer("insert_caller_unique_identity", insertCallerUniqueIdentity, "data")
	server.RegisterReducer("insert_caller_pk_identity", insertCallerPkIdentity, "data")
	server.RegisterReducer("insert_caller_one_connection_id", insertCallerOneConnectionId)
	server.RegisterReducer("insert_caller_vec_connection_id", insertCallerVecConnectionId)
	server.RegisterReducer("insert_caller_unique_connection_id", insertCallerUniqueConnectionId, "data")
	server.RegisterReducer("insert_caller_pk_connection_id", insertCallerPkConnectionId, "data")

	// Timestamp and UUID reducers (no params)
	server.RegisterReducer("insert_call_timestamp", insertCallTimestamp)
	server.RegisterReducer("insert_call_uuid_v4", insertCallUuidV4)
	server.RegisterReducer("insert_call_uuid_v7", insertCallUuidV7)

	// Primitives-as-strings
	server.RegisterReducer("insert_primitives_as_strings", insertPrimitivesAsStrings, "s")

	// Misc
	server.RegisterReducer("no_op_succeeds", noOpSucceeds)
	server.RegisterReducer("send_scheduled_message", sendScheduledMessage, "arg")
	server.RegisterReducer("insert_user", insertUser, "name", "identity")
	server.RegisterReducer("insert_into_indexed_simple_enum", insertIntoIndexedSimpleEnum, "n")
	server.RegisterReducer("update_indexed_simple_enum", updateIndexedSimpleEnum, "a", "b")
	server.RegisterReducer("sorted_uuids_insert", sortedUuidsInsert)

	// -----------------------------------------------------------------
	// 4. Row Level Security (client visibility) filters
	// -----------------------------------------------------------------
	server.RegisterRowLevelSecurity("SELECT * FROM one_u_8")
	server.RegisterRowLevelSecurity("SELECT * FROM users WHERE identity = :sender")
}
