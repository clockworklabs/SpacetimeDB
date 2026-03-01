package runtime

import (
	"fmt"
	"reflect"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// SumTypeVariantDef describes a single variant of a registered sum type.
type SumTypeVariantDef struct {
	Name string       // Variant name (e.g., "U8", "Str")
	Type reflect.Type // Go concrete type for this variant
}

// sumTypeInfo holds the complete registration for one sum type interface.
type sumTypeInfo struct {
	ifaceType reflect.Type
	variants  []SumTypeVariantDef
	// Map from concrete type to variant index for fast lookup during encoding.
	typeToIdx map[reflect.Type]int
}

// Global registry for sum types.
// Registration happens exclusively during init() which is single-threaded in Go.
// Lookups happen during module execution which is single-threaded in WASM.
// No synchronization is needed.
var (
	sumTypes        []sumTypeInfo
	sumTypesByIface = make(map[reflect.Type]int) // ifaceType -> index in sumTypes
)

// RegisterSumType registers a sum type interface with its variants.
// The ifaceType should be the reflect.Type of the interface (e.g., reflect.TypeOf((*MyEnum)(nil)).Elem()).
// Variants must be registered in order (tag 0, 1, 2, ...).
// Each variant's concrete type must implement the interface.
// Must be called during init() only.
func RegisterSumType(ifaceType reflect.Type, variants []SumTypeVariantDef) {
	if ifaceType.Kind() != reflect.Interface {
		panic(fmt.Sprintf("runtime.RegisterSumType: expected interface type, got %v", ifaceType))
	}

	info := sumTypeInfo{
		ifaceType: ifaceType,
		variants:  variants,
		typeToIdx: make(map[reflect.Type]int, len(variants)),
	}

	for i, v := range variants {
		if !v.Type.Implements(ifaceType) {
			panic(fmt.Sprintf("runtime.RegisterSumType: variant type %v does not implement %v", v.Type, ifaceType))
		}
		info.typeToIdx[v.Type] = i
	}

	idx := len(sumTypes)
	sumTypes = append(sumTypes, info)
	sumTypesByIface[ifaceType] = idx
}

// lookupSumType returns the sum type info for an interface type, or nil if not registered.
func lookupSumType(ifaceType reflect.Type) *sumTypeInfo {
	if idx, ok := sumTypesByIface[ifaceType]; ok {
		return &sumTypes[idx]
	}
	return nil
}

// sumTypeAlgebraic returns the AlgebraicType (SumType) for a registered sum type interface.
func sumTypeAlgebraic(info *sumTypeInfo) types.AlgebraicType {
	variants := make([]types.SumTypeVariant, len(info.variants))
	for i, v := range info.variants {
		variants[i] = types.SumTypeVariant{
			Name:          v.Name,
			AlgebraicType: deriveVariantAlgType(v.Type),
		}
	}
	return types.AlgTypeSum(types.NewSumType(variants...))
}

// deriveVariantAlgType derives the AlgebraicType for a variant's payload
// from its struct's exported fields.
func deriveVariantAlgType(vt reflect.Type) types.AlgebraicType {
	if vt.Kind() != reflect.Struct {
		panic(fmt.Sprintf("runtime: sum type variant must be a struct type, got %v", vt))
	}

	var exported []reflect.StructField
	for i := 0; i < vt.NumField(); i++ {
		if vt.Field(i).IsExported() {
			exported = append(exported, vt.Field(i))
		}
	}

	if len(exported) == 0 {
		// Unit variant — empty product.
		return types.AlgTypeProduct(types.NewProductType())
	}

	if len(exported) == 1 {
		// Single-field variant — the payload is the field type.
		return goTypeToAlgebraic(exported[0].Type)
	}

	// Multi-field variant — the payload is a product of all fields.
	elements := make([]types.ProductTypeElement, len(exported))
	for i, f := range exported {
		elements[i] = types.ProductTypeElement{
			Name:          toSnakeCase(f.Name),
			AlgebraicType: goTypeToAlgebraic(f.Type),
		}
	}
	return types.AlgTypeProduct(types.NewProductType(elements...))
}

// simpleEnumInfo holds the registration for a C-style enum (named integer type
// with unit variants). In SATS, these are sum types where each variant has an
// empty product payload. The BSATN encoding is a u8 tag — identical to encoding
// the underlying integer value.
type simpleEnumInfo struct {
	goType   reflect.Type
	variants []string // variant names in tag order
}

// Global registry for simple enums.
var (
	simpleEnums       []simpleEnumInfo
	simpleEnumsByType = make(map[reflect.Type]int) // goType -> index in simpleEnums
)

// RegisterSimpleEnum registers a named integer type as a sum type with unit
// variants. The variants are the names of the enum constants in tag order
// (0, 1, 2, ...). The Go type must be a named type based on an integer kind
// (e.g., `type SimpleEnum uint8`).
// Must be called during init() only.
func RegisterSimpleEnum(goType reflect.Type, variants ...string) {
	if len(variants) == 0 {
		panic(fmt.Sprintf("runtime.RegisterSimpleEnum: type %v must have at least one variant", goType))
	}

	info := simpleEnumInfo{
		goType:   goType,
		variants: variants,
	}

	idx := len(simpleEnums)
	simpleEnums = append(simpleEnums, info)
	simpleEnumsByType[goType] = idx
}

// lookupSimpleEnum returns the simple enum info for a Go type, or nil if not registered.
func lookupSimpleEnum(t reflect.Type) *simpleEnumInfo {
	if idx, ok := simpleEnumsByType[t]; ok {
		return &simpleEnums[idx]
	}
	return nil
}

// simpleEnumAlgebraic returns the AlgebraicType (SumType) for a registered simple enum.
// Each variant is a unit variant (empty product payload).
func simpleEnumAlgebraic(info *simpleEnumInfo) types.AlgebraicType {
	variants := make([]types.SumTypeVariant, len(info.variants))
	for i, name := range info.variants {
		variants[i] = types.SumTypeVariant{
			Name:          name,
			AlgebraicType: types.AlgTypeProduct(types.NewProductType()),
		}
	}
	return types.AlgTypeSum(types.NewSumType(variants...))
}
