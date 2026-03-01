package moduledef

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// TypeDef defines a type declaration in the module.
type TypeDef interface {
	bsatn.Serializable
}

// TypeDefBuilder builds a TypeDef.
type TypeDefBuilder interface {
	WithCustomOrdering(v bool) TypeDefBuilder
	Build() TypeDef
}

// NewTypeDefBuilder creates a TypeDefBuilder.
// scope is the scope segments (empty for no scope).
// sourceName is the type name.
// typeRef is the AlgebraicTypeRef pointing to this type in the typespace.
func NewTypeDefBuilder(scope []string, sourceName string, typeRef types.TypeRef) TypeDefBuilder {
	return &typeDef{
		scope:      scope,
		sourceName: sourceName,
		typeRef:    typeRef,
	}
}

type typeDef struct {
	scope          []string
	sourceName     string
	typeRef        types.TypeRef
	customOrdering bool
}

func (t *typeDef) WithCustomOrdering(v bool) TypeDefBuilder {
	t.customOrdering = v
	return t
}

func (t *typeDef) Build() TypeDef {
	return t
}

// WriteBsatn encodes the type definition as BSATN.
//
// Matches RawTypeDefV10 product field order:
//
//	source_name: RawScopedTypeNameV10
//	ty: AlgebraicTypeRef (u32)
//	custom_ordering: bool
//
// RawScopedTypeNameV10 product:
//
//	scope: Box<[String]> (array of strings)
//	source_name: String
func (t *typeDef) WriteBsatn(w bsatn.Writer) {
	// source_name: RawScopedTypeNameV10 (product)
	// scope: array of strings
	w.PutArrayLen(uint32(len(t.scope)))
	for _, s := range t.scope {
		w.PutString(s)
	}
	// source_name: String
	w.PutString(t.sourceName)

	// ty: AlgebraicTypeRef (u32)
	w.PutU32(uint32(t.typeRef))

	// custom_ordering: bool
	w.PutBool(t.customOrdering)
}
