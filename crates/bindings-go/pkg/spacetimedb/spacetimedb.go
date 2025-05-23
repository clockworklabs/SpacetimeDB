// Package spacetimedb provides Go bindings for SpacetimeDB.
//
// This package includes core types, reducer framework, schema management,
// BSATN serialization, and utilities for building SpacetimeDB modules in Go.
//
// Core Types:
//   - Identity: Player identity
//   - Timestamp: Point in time
//   - TimeDuration: Duration of time
//   - ScheduleAt: Scheduling specification
//
// Reducer Framework:
//   - ReducerFunction: Interface for reducers
//   - LifecycleFunction: Interface for lifecycle functions
//   - ReducerRegistry: Registry for managing reducers
//   - GenericReducer: Simple reducer implementation
//   - GenericLifecycleFunction: Simple lifecycle function implementation
//
// Schema Management:
//   - TableInfo: Table definition structure
//   - Column: Column definition structure
//   - Index: Index definition structure
//   - TableRegistry: Registry for table definitions
//
// BSATN Serialization:
//   - High-performance binary serialization format
//   - Support for all primitive types (u8, u16, u32, u64, i8, i16, i32, i64, f32, f64, bool, string, bytes)
//   - Advanced collections (arrays, maps, optionals)
//   - Generic encoding/decoding functions
//   - Size calculation utilities
//
// Example usage:
//
//	import "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb"
//
//	// Use core types
//	identity := spacetimedb.NewIdentity([16]byte{...})
//	timestamp := spacetimedb.NewTimestampFromTime(time.Now())
//
//	// Create table schema
//	table := spacetimedb.NewTableInfo("users")
//	table.Columns = []spacetimedb.TableColumn{
//		spacetimedb.NewPrimaryKeyColumn("id", spacetimedb.TypeU32),
//		spacetimedb.NewColumn("name", spacetimedb.TypeString),
//	}
//	spacetimedb.GlobalRegisterTable(table)
//
//	// Create and register a reducer
//	myReducer := spacetimedb.NewGenericReducer("my_reducer", "Does something", func(ctx *spacetimedb.ReducerContext, args []byte) spacetimedb.ReducerResult {
//		// Your reducer logic here
//		return spacetimedb.NewSuccessResult()
//	})
//	spacetimedb.RegisterReducer(myReducer)
package spacetimedb

// Re-export all components for convenience
import (
	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb/bsatn"
	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb/reducers"
	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb/schema"
	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb/types"
)

// Core SpacetimeDB types
type (
	// Identity represents a unique player identity
	Identity = types.Identity

	// Timestamp represents a point in time
	Timestamp = types.Timestamp

	// TimeDuration represents a duration of time
	TimeDuration = types.TimeDuration

	// ScheduleAt represents when a scheduled reducer should be executed
	ScheduleAt = types.ScheduleAt
)

// Reducer framework types
type (
	// LifecycleType represents the type of lifecycle event
	LifecycleType = reducers.LifecycleType

	// ReducerContext contains information about the current reducer execution
	ReducerContext = reducers.ReducerContext

	// ReducerResult represents the result of a reducer execution
	ReducerResult = reducers.ReducerResult

	// ReducerFunction represents a function that can be called as a reducer
	ReducerFunction = reducers.ReducerFunction

	// LifecycleFunction represents a function that can be called for lifecycle events
	LifecycleFunction = reducers.LifecycleFunction

	// GenericReducer provides a simple implementation of ReducerFunction
	GenericReducer = reducers.GenericReducer

	// GenericLifecycleFunction provides a simple implementation of LifecycleFunction
	GenericLifecycleFunction = reducers.GenericLifecycleFunction

	// ReducerRegistry manages all registered reducers and lifecycle functions
	ReducerRegistry = reducers.ReducerRegistry

	// ReducerMetrics tracks performance metrics for reducers
	ReducerMetrics = reducers.ReducerMetrics
)

// Schema framework types
type (
	// TableInfo contains metadata about a SpacetimeDB table
	TableInfo = schema.TableInfo

	// TableColumn represents a table column definition
	TableColumn = schema.Column

	// TableIndex represents a table index definition
	TableIndex = schema.Index

	// TableRegistry manages a collection of table definitions
	TableRegistry = schema.TableRegistry

	// SchemaTableID represents a unique identifier for a SpacetimeDB table in schema
	SchemaTableID = schema.TableID

	// ColumnID represents a unique identifier for a table column
	ColumnID = schema.ColumnID

	// SchemaIndexID represents a unique identifier for a table index in schema
	SchemaIndexID = schema.IndexID

	// IndexType represents the type of index
	IndexType = schema.IndexType

	// TableRegistrationOptions configures table registration behavior
	TableRegistrationOptions = schema.RegistrationOptions

	// TableRegistryStats contains statistics about a table registry
	TableRegistryStats = schema.RegistryStats
)

// Lifecycle constants
const (
	LifecycleInit       = reducers.LifecycleInit
	LifecycleUpdate     = reducers.LifecycleUpdate
	LifecycleConnect    = reducers.LifecycleConnect
	LifecycleDisconnect = reducers.LifecycleDisconnect
)

// Schema constants - SpacetimeDB type names
const (
	TypeU8           = schema.TypeU8
	TypeU16          = schema.TypeU16
	TypeU32          = schema.TypeU32
	TypeU64          = schema.TypeU64
	TypeU128         = schema.TypeU128
	TypeU256         = schema.TypeU256
	TypeI8           = schema.TypeI8
	TypeI16          = schema.TypeI16
	TypeI32          = schema.TypeI32
	TypeI64          = schema.TypeI64
	TypeI128         = schema.TypeI128
	TypeI256         = schema.TypeI256
	TypeF32          = schema.TypeF32
	TypeF64          = schema.TypeF64
	TypeBool         = schema.TypeBool
	TypeString       = schema.TypeString
	TypeBytes        = schema.TypeBytes
	TypeIdentity     = schema.TypeIdentity
	TypeTimestamp    = schema.TypeTimestamp
	TypeTimeDuration = schema.TypeTimeDuration
	TypeScheduleAt   = schema.TypeScheduleAt
)

// Index type constants
const (
	IndexTypeBTree  = schema.IndexTypeBTree
	IndexTypeHash   = schema.IndexTypeHash
	IndexTypeDirect = schema.IndexTypeDirect
)

// Core type constructors
var (
	// Identity constructors
	NewIdentity = types.NewIdentity

	// Timestamp constructors
	NewTimestamp         = types.NewTimestamp
	NewTimestampFromTime = types.NewTimestampFromTime

	// TimeDuration constructors
	NewTimeDuration             = types.NewTimeDuration
	NewTimeDurationFromDuration = types.NewTimeDurationFromDuration

	// ScheduleAt constructors
	NewScheduleAtTime     = types.NewScheduleAtTime
	NewScheduleAtInterval = types.NewScheduleAtInterval
)

// Reducer result constructors
var (
	NewSuccessResult            = reducers.NewSuccessResult
	NewSuccessResultWithMessage = reducers.NewSuccessResultWithMessage
	NewErrorResult              = reducers.NewErrorResult
	NewErrorResultWithMessage   = reducers.NewErrorResultWithMessage
)

// Reducer and lifecycle constructors
var (
	NewGenericReducer           = reducers.NewGenericReducer
	NewGenericLifecycleFunction = reducers.NewGenericLifecycleFunction
	NewReducerRegistry          = reducers.NewReducerRegistry
	NewReducerMetrics           = reducers.NewReducerMetrics
)

// Schema constructors
var (
	// Table constructors
	NewTableInfo     = schema.NewTableInfo
	NewTableRegistry = schema.NewTableRegistry

	// Column constructors
	NewColumn           = schema.NewColumn
	NewPrimaryKeyColumn = schema.NewPrimaryKeyColumn
	NewAutoIncColumn    = schema.NewAutoIncColumn

	// Index constructors
	NewIndex       = schema.NewIndex
	NewBTreeIndex  = schema.NewBTreeIndex
	NewUniqueIndex = schema.NewUniqueIndex

	// Options constructor
	DefaultRegistrationOptions = schema.DefaultRegistrationOptions
)

// Global registry functions - Reducers
var (
	RegisterReducer           = reducers.RegisterReducer
	RegisterLifecycleFunction = reducers.RegisterLifecycleFunction
	GetReducer                = reducers.GetReducer
	GetLifecycleFunction      = reducers.GetLifecycleFunction
)

// Global registry functions - Tables
var (
	GlobalRegisterTable     = schema.GlobalRegister
	GlobalRegisterAllTables = schema.GlobalRegisterAll
	GlobalGetTable          = schema.GlobalGetTable
	GlobalGetTableByID      = schema.GlobalGetTableByID
	GlobalMustGetTable      = schema.GlobalMustGetTable
	GlobalHasTable          = schema.GlobalHasTable
	GlobalGetAllTables      = schema.GlobalGetAllTables
	GlobalGetTableNames     = schema.GlobalGetTableNames
	GlobalTableCount        = schema.GlobalCount
	GlobalClearTables       = schema.GlobalClear
	GlobalValidateAllTables = schema.GlobalValidateAll
	GlobalGetTableStats     = schema.GlobalGetStats
	GetGlobalTableRegistry  = schema.GetGlobalRegistry
)

// BSATN serialization functions
var (
	// Primitive type encoders
	BsatnEncodeU8     = bsatn.EncodeU8
	BsatnEncodeU16    = bsatn.EncodeU16
	BsatnEncodeU32    = bsatn.EncodeU32
	BsatnEncodeU64    = bsatn.EncodeU64
	BsatnEncodeI8     = bsatn.EncodeI8
	BsatnEncodeI16    = bsatn.EncodeI16
	BsatnEncodeI32    = bsatn.EncodeI32
	BsatnEncodeI64    = bsatn.EncodeI64
	BsatnEncodeF32    = bsatn.EncodeF32
	BsatnEncodeF64    = bsatn.EncodeF64
	BsatnEncodeBool   = bsatn.EncodeBool
	BsatnEncodeString = bsatn.EncodeString
	BsatnEncodeBytes  = bsatn.EncodeBytes

	// Primitive type decoders
	BsatnDecodeU8     = bsatn.DecodeU8
	BsatnDecodeU16    = bsatn.DecodeU16
	BsatnDecodeU32    = bsatn.DecodeU32
	BsatnDecodeU64    = bsatn.DecodeU64
	BsatnDecodeI8     = bsatn.DecodeI8
	BsatnDecodeI16    = bsatn.DecodeI16
	BsatnDecodeI32    = bsatn.DecodeI32
	BsatnDecodeI64    = bsatn.DecodeI64
	BsatnDecodeF32    = bsatn.DecodeF32
	BsatnDecodeF64    = bsatn.DecodeF64
	BsatnDecodeBool   = bsatn.DecodeBool
	BsatnDecodeString = bsatn.DecodeString
	BsatnDecodeBytes  = bsatn.DecodeBytes

	// Array encoders/decoders
	BsatnEncodeU32Array    = bsatn.EncodeU32Array
	BsatnDecodeU32Array    = bsatn.DecodeU32Array
	BsatnEncodeStringArray = bsatn.EncodeStringArray
	BsatnDecodeStringArray = bsatn.DecodeStringArray
	BsatnEncodeF64Array    = bsatn.EncodeF64Array
	BsatnDecodeF64Array    = bsatn.DecodeF64Array

	// Generic collection functions
	BsatnEncodeArray    = bsatn.EncodeArray[any]
	BsatnDecodeArray    = bsatn.DecodeArray[any]
	BsatnEncodeOptional = bsatn.EncodeOptional[any]
	BsatnDecodeOptional = bsatn.DecodeOptional[any]
	BsatnEncodeMap      = bsatn.EncodeMap[string, any]
	BsatnDecodeMap      = bsatn.DecodeMap[string, any]

	// Size calculation functions
	BsatnSizeU8          = bsatn.SizeU8
	BsatnSizeU16         = bsatn.SizeU16
	BsatnSizeU32         = bsatn.SizeU32
	BsatnSizeU64         = bsatn.SizeU64
	BsatnSizeI8          = bsatn.SizeI8
	BsatnSizeI16         = bsatn.SizeI16
	BsatnSizeI32         = bsatn.SizeI32
	BsatnSizeI64         = bsatn.SizeI64
	BsatnSizeF32         = bsatn.SizeF32
	BsatnSizeF64         = bsatn.SizeF64
	BsatnSizeBool        = bsatn.SizeBool
	BsatnSizeString      = bsatn.SizeString
	BsatnSizeBytes       = bsatn.SizeBytes
	BsatnSizeU32Array    = bsatn.SizeU32Array
	BsatnSizeStringArray = bsatn.SizeStringArray
	BsatnSizeF64Array    = bsatn.SizeF64Array

	// Utility functions
	BsatnToBytes              = bsatn.ToBytes
	BsatnFromBytes            = bsatn.FromBytes
	BsatnU32ArrayToBytes      = bsatn.U32ArrayToBytes
	BsatnU32ArrayFromBytes    = bsatn.U32ArrayFromBytes
	BsatnStringArrayToBytes   = bsatn.StringArrayToBytes
	BsatnStringArrayFromBytes = bsatn.StringArrayFromBytes
)

// Package information
const (
	Version = "0.3.0"
	Name    = "SpacetimeDB Go Bindings"
)

// GetVersion returns the current version of the bindings
func GetVersion() string {
	return Version
}

// GetPackageName returns the package name
func GetPackageName() string {
	return Name
}

// GetFeatures returns a list of supported features
func GetFeatures() []string {
	return []string{
		"Core Types (Identity, Timestamp, TimeDuration, ScheduleAt)",
		"Reducer Framework (Registration, Execution, Metrics)",
		"Schema Management (Tables, Columns, Indexes)",
		"BSATN Serialization (Binary, High-Performance)",
		"Collection Support (Arrays, Maps, Optionals)",
		"Global Registries (Thread-safe, Validation)",
		"JSON Serialization",
		"WASM Support",
	}
}
