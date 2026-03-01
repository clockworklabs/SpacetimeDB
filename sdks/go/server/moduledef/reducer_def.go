package moduledef

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// FunctionVisibility defines reducer/procedure visibility.
// BSATN enum: Private=0, ClientCallable=1.
type FunctionVisibility uint8

const (
	FunctionVisibilityPrivate        FunctionVisibility = 0
	FunctionVisibilityClientCallable FunctionVisibility = 1
)

// ReducerDef defines a reducer in the module.
type ReducerDef interface {
	bsatn.Serializable
}

// ReducerDefBuilder builds a ReducerDef.
type ReducerDefBuilder interface {
	WithParams(params types.ProductType) ReducerDefBuilder
	WithVisibility(v FunctionVisibility) ReducerDefBuilder
	WithOkReturnType(t types.AlgebraicType) ReducerDefBuilder
	WithErrReturnType(t types.AlgebraicType) ReducerDefBuilder
	Build() ReducerDef
}

// NewReducerDefBuilder creates a ReducerDefBuilder with the given source name.
func NewReducerDefBuilder(sourceName string) ReducerDefBuilder {
	return &reducerDef{
		sourceName: sourceName,
		visibility: FunctionVisibilityClientCallable,
	}
}

type reducerDef struct {
	sourceName    string
	params        types.ProductType
	visibility    FunctionVisibility
	okReturnType  types.AlgebraicType
	errReturnType types.AlgebraicType
}

func (r *reducerDef) WithParams(params types.ProductType) ReducerDefBuilder {
	r.params = params
	return r
}

func (r *reducerDef) WithVisibility(v FunctionVisibility) ReducerDefBuilder {
	r.visibility = v
	return r
}

func (r *reducerDef) WithOkReturnType(t types.AlgebraicType) ReducerDefBuilder {
	r.okReturnType = t
	return r
}

func (r *reducerDef) WithErrReturnType(t types.AlgebraicType) ReducerDefBuilder {
	r.errReturnType = t
	return r
}

func (r *reducerDef) Build() ReducerDef {
	return r
}

// WriteBsatn encodes the reducer definition as BSATN.
//
// Matches RawReducerDefV10 product field order:
//
//	source_name: String
//	params: ProductType
//	visibility: FunctionVisibility (sum tag)
//	ok_return_type: AlgebraicType
//	err_return_type: AlgebraicType
func (r *reducerDef) WriteBsatn(w bsatn.Writer) {
	// source_name: String
	w.PutString(r.sourceName)

	// params: ProductType
	if r.params != nil {
		r.params.WriteBsatn(w)
	} else {
		// Empty product type: 0 elements
		w.PutArrayLen(0)
	}

	// visibility: FunctionVisibility (sum tag)
	w.PutSumTag(uint8(r.visibility))

	// ok_return_type: AlgebraicType
	if r.okReturnType != nil {
		r.okReturnType.WriteBsatn(w)
	} else {
		// Default: empty product type (unit) - tag 2 for Product, then empty elements
		types.AlgTypeProduct(types.NewProductType()).WriteBsatn(w)
	}

	// err_return_type: AlgebraicType
	if r.errReturnType != nil {
		r.errReturnType.WriteBsatn(w)
	} else {
		// Default: empty product type (unit)
		types.AlgTypeProduct(types.NewProductType()).WriteBsatn(w)
	}
}

// Lifecycle defines lifecycle event types.
// BSATN enum: Init=0, OnConnect=1, OnDisconnect=2.
type Lifecycle uint8

const (
	LifecycleInit         Lifecycle = 0
	LifecycleOnConnect    Lifecycle = 1
	LifecycleOnDisconnect Lifecycle = 2
)

// LifecycleReducerDef defines a lifecycle reducer assignment.
type LifecycleReducerDef interface {
	bsatn.Serializable
}

// NewLifecycleReducerDef creates a LifecycleReducerDef.
func NewLifecycleReducerDef(lifecycle Lifecycle, functionName string) LifecycleReducerDef {
	return &lifecycleReducerDef{
		lifecycle:    lifecycle,
		functionName: functionName,
	}
}

type lifecycleReducerDef struct {
	lifecycle    Lifecycle
	functionName string
}

// WriteBsatn encodes the lifecycle reducer definition as BSATN.
//
// Matches RawLifeCycleReducerDefV10 product field order:
//
//	lifecycle_spec: Lifecycle (sum tag)
//	function_name: String
func (lr *lifecycleReducerDef) WriteBsatn(w bsatn.Writer) {
	// lifecycle_spec: Lifecycle (sum tag)
	w.PutSumTag(uint8(lr.lifecycle))

	// function_name: String
	w.PutString(lr.functionName)
}
