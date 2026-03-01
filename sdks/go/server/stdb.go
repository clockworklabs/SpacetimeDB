// Package server provides top-level convenience re-exports for SpacetimeDB
// Go module authoring. Import this package to access registration functions,
// reducer context types, table access constants, and logging.
package server

import (
	"reflect"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/log"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/moduledef"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/reducer"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/runtime"
)

// Table access constants.
const (
	TableAccessPublic  = moduledef.TableAccessPublic
	TableAccessPrivate = moduledef.TableAccessPrivate
)

// Lifecycle constants.
const (
	LifecycleInit               = reducer.LifecycleInit
	LifecycleClientConnected    = reducer.LifecycleClientConnected
	LifecycleClientDisconnected = reducer.LifecycleClientDisconnected
)

// Type aliases for commonly used interfaces.
type (
	ReducerContext      = reducer.ReducerContext
	TableRegistration   = runtime.TableRegistration
	ReducerRegistration = runtime.ReducerRegistration
	SumTypeVariantDef   = runtime.SumTypeVariantDef
)

// RegisterTable registers a table type. Called during init().
// T must be a struct. Schema is discovered via reflect.
func RegisterTable[T any](name string, access moduledef.TableAccess) TableRegistration {
	return runtime.RegisterTable[T](name, access)
}

// RegisterReducer registers a reducer function. Called during init().
// Optional paramNames specify names for the reducer parameters (after ReducerContext).
// If not provided, parameters are named "arg_0", "arg_1", etc.
func RegisterReducer(name string, fn any, paramNames ...string) ReducerRegistration {
	return runtime.RegisterReducer(name, fn, paramNames...)
}

// RegisterLifecycleReducer registers a lifecycle reducer. Called during init().
func RegisterLifecycleReducer(lifecycle reducer.Lifecycle, fn any) {
	runtime.RegisterLifecycleReducer(lifecycle, fn)
}

// NewLogger creates a Logger that writes to the host console.
func NewLogger(target string) log.Logger {
	return log.NewLogger(target)
}

// Variant creates a SumTypeVariantDef for a concrete variant type.
// T must be a struct type that implements the sum type interface.
// Example: server.Variant[EnumWithPayloadU8]("U8")
func Variant[T any](name string) SumTypeVariantDef {
	var zero T
	return SumTypeVariantDef{
		Name: name,
		Type: reflect.TypeOf(zero),
	}
}

// RegisterSumType registers a sum type interface with its variants.
// I must be an interface type. Variants must be provided in tag order (0, 1, 2, ...).
// Example:
//
//	server.RegisterSumType[EnumWithPayload](
//	    server.Variant[EnumWithPayloadU8]("U8"),
//	    server.Variant[EnumWithPayloadU16]("U16"),
//	)
func RegisterSumType[I any](variants ...SumTypeVariantDef) {
	runtime.RegisterSumType(reflect.TypeOf((*I)(nil)).Elem(), variants)
}

// RegisterRowLevelSecurity registers an RLS (client visibility) filter SQL string.
// Called during init(). Each filter is a SQL SELECT that determines which rows
// a client can see. Use :sender to refer to the client's identity.
// Example: server.RegisterRowLevelSecurity("SELECT * FROM users WHERE identity = :sender")
func RegisterRowLevelSecurity(sql string) {
	runtime.RegisterRowLevelSecurity(sql)
}

// RegisterSimpleEnum registers a named integer type as a sum type with unit
// variants (a "C-style enum"). The variants are the names of the enum constants
// in tag order (0, 1, 2, ...).
// Example: server.RegisterSimpleEnum[MyEnum]("VariantA", "VariantB", "VariantC")
func RegisterSimpleEnum[T ~uint8 | ~uint16 | ~uint32 | ~uint64 | ~int8 | ~int16 | ~int32 | ~int64](variants ...string) {
	var zero T
	runtime.RegisterSimpleEnum(reflect.TypeOf(zero), variants...)
}
