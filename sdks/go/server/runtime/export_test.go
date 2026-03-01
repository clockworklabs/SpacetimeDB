package runtime

import (
	"reflect"
	"unsafe"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/reducer"
)

// ExportedStructPlan exposes structPlan for testing.
type ExportedStructPlan = structPlan

// ExportedBuildStructPlan exposes buildStructPlan for testing.
func ExportedBuildStructPlan(t reflect.Type) *ExportedStructPlan {
	return buildStructPlan(t)
}

// PlanEncode is an exported method for testing.
func (p *structPlan) PlanEncode(w bsatn.Writer, base unsafe.Pointer) {
	p.planEncode(w, base)
}

// PlanDecode is an exported method for testing.
func (p *structPlan) PlanDecode(r bsatn.Reader, base unsafe.Pointer) error {
	return p.planDecode(r, base)
}

// ExportedReflectEncodeValue exposes reflectEncodeValue for testing.
func ExportedReflectEncodeValue(w bsatn.Writer, rv reflect.Value) {
	reflectEncodeValue(w, rv)
}

// ExportedGetReducerDispatch returns the dispatch function for a named reducer.
func ExportedGetReducerDispatch(name string) reducer.ReducerFunc {
	for i := range registeredReducers {
		if registeredReducers[i].name == name {
			return registeredReducers[i].dispatchFn
		}
	}
	return nil
}

// ExportedClearReducers clears all registered reducers (for test isolation).
func ExportedClearReducers() {
	registeredReducers = registeredReducers[:0]
}

// ExportedBuildParamDecoder exposes buildParamDecoder for testing.
func ExportedBuildParamDecoder(pt reflect.Type) func(r bsatn.Reader, ptr unsafe.Pointer) error {
	return buildParamDecoder(pt)
}
