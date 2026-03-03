package moduledef

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// ViewDef defines a view in the module.
type ViewDef interface {
	bsatn.Serializable
}

// ViewDefBuilder builds a ViewDef.
type ViewDefBuilder interface {
	WithIndex(index uint32) ViewDefBuilder
	WithIsPublic(isPublic bool) ViewDefBuilder
	WithIsAnonymous(isAnonymous bool) ViewDefBuilder
	WithParams(params types.ProductType) ViewDefBuilder
	WithReturnType(returnType types.AlgebraicType) ViewDefBuilder
	Build() ViewDef
}

// NewViewDefBuilder creates a ViewDefBuilder with the given source name.
func NewViewDefBuilder(sourceName string) ViewDefBuilder {
	return &viewDef{
		sourceName: sourceName,
	}
}

type viewDef struct {
	sourceName  string
	index       uint32
	isPublic    bool
	isAnonymous bool
	params      types.ProductType
	returnType  types.AlgebraicType
}

func (v *viewDef) WithIndex(index uint32) ViewDefBuilder {
	v.index = index
	return v
}

func (v *viewDef) WithIsPublic(isPublic bool) ViewDefBuilder {
	v.isPublic = isPublic
	return v
}

func (v *viewDef) WithIsAnonymous(isAnonymous bool) ViewDefBuilder {
	v.isAnonymous = isAnonymous
	return v
}

func (v *viewDef) WithParams(params types.ProductType) ViewDefBuilder {
	v.params = params
	return v
}

func (v *viewDef) WithReturnType(returnType types.AlgebraicType) ViewDefBuilder {
	v.returnType = returnType
	return v
}

func (v *viewDef) Build() ViewDef {
	return v
}

// WriteBsatn encodes the view definition as BSATN.
//
// Matches RawViewDefV10 product field order:
//
//	source_name: String
//	index: u32
//	is_public: bool
//	is_anonymous: bool
//	params: ProductType
//	return_type: AlgebraicType
func (v *viewDef) WriteBsatn(w bsatn.Writer) {
	// source_name: String
	w.PutString(v.sourceName)

	// index: u32
	w.PutU32(v.index)

	// is_public: bool
	w.PutBool(v.isPublic)

	// is_anonymous: bool
	w.PutBool(v.isAnonymous)

	// params: ProductType
	if v.params != nil {
		v.params.WriteBsatn(w)
	} else {
		w.PutArrayLen(0)
	}

	// return_type: AlgebraicType
	if v.returnType != nil {
		v.returnType.WriteBsatn(w)
	} else {
		types.AlgTypeProduct(types.NewProductType()).WriteBsatn(w)
	}
}
