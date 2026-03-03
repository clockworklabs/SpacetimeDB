package moduledef

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// ProcedureDef defines a procedure in the module.
type ProcedureDef interface {
	bsatn.Serializable
}

// ProcedureDefBuilder builds a ProcedureDef.
type ProcedureDefBuilder interface {
	WithParams(params types.ProductType) ProcedureDefBuilder
	WithReturnType(returnType types.AlgebraicType) ProcedureDefBuilder
	WithVisibility(v FunctionVisibility) ProcedureDefBuilder
	Build() ProcedureDef
}

// NewProcedureDefBuilder creates a ProcedureDefBuilder with the given source name.
func NewProcedureDefBuilder(sourceName string) ProcedureDefBuilder {
	return &procedureDef{
		sourceName: sourceName,
		visibility: FunctionVisibilityClientCallable,
	}
}

type procedureDef struct {
	sourceName string
	params     types.ProductType
	returnType types.AlgebraicType
	visibility FunctionVisibility
}

func (p *procedureDef) WithParams(params types.ProductType) ProcedureDefBuilder {
	p.params = params
	return p
}

func (p *procedureDef) WithReturnType(returnType types.AlgebraicType) ProcedureDefBuilder {
	p.returnType = returnType
	return p
}

func (p *procedureDef) WithVisibility(v FunctionVisibility) ProcedureDefBuilder {
	p.visibility = v
	return p
}

func (p *procedureDef) Build() ProcedureDef {
	return p
}

// WriteBsatn encodes the procedure definition as BSATN.
//
// Matches RawProcedureDefV10 product field order:
//
//	source_name: String
//	params: ProductType
//	return_type: AlgebraicType
//	visibility: FunctionVisibility (sum tag)
func (p *procedureDef) WriteBsatn(w bsatn.Writer) {
	// source_name: String
	w.PutString(p.sourceName)

	// params: ProductType
	if p.params != nil {
		p.params.WriteBsatn(w)
	} else {
		w.PutArrayLen(0)
	}

	// return_type: AlgebraicType
	if p.returnType != nil {
		p.returnType.WriteBsatn(w)
	} else {
		types.AlgTypeProduct(types.NewProductType()).WriteBsatn(w)
	}

	// visibility: FunctionVisibility (sum tag)
	w.PutSumTag(uint8(p.visibility))
}
