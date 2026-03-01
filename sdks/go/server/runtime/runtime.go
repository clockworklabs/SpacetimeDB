package runtime

import (
	"fmt"
	"reflect"
	"unsafe"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/moduledef"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/reducer"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// TableRegistration represents a registered table (opaque).
type TableRegistration interface {
	Name() string
}

// ReducerRegistration represents a registered reducer (opaque).
type ReducerRegistration interface {
	Name() string
}

// Module-level registry populated during init() time.
var (
	registeredTables    []tableRegistration
	registeredReducers  []reducerRegistration
	registeredLifecycle []lifecycleRegistration
	registeredRLS       []string
)

// RegisterRowLevelSecurity registers an RLS (client visibility) filter SQL string.
// Called during init(). Each filter is a SQL SELECT that determines which rows
// a client can see. Use :sender to refer to the client's identity.
// Example: "SELECT * FROM users WHERE identity = :sender"
func RegisterRowLevelSecurity(sql string) {
	registeredRLS = append(registeredRLS, sql)
}

// tableRegistration holds all metadata for a registered table.
type tableRegistration struct {
	name            string
	access          moduledef.TableAccess
	schema          structSchema
	typeRef         types.TypeRef
	goType          reflect.Type
	plan            *structPlan
	encodeFn        func(v any) []byte
	decodeFn        func(data []byte) (any, error)
	decodeReaderFn  func(r bsatn.Reader) (any, error)
}

func (r *tableRegistration) Name() string { return r.name }

// reducerRegistration holds all metadata for a registered reducer.
type reducerRegistration struct {
	name              string
	fn                any
	paramType         types.ProductType
	paramReflectTypes []reflect.Type // Go reflect types for each parameter (for deferred TypeRef resolution)
	paramNames        []string       // Optional explicit names for parameters (snake_case)
	dispatchFn        reducer.ReducerFunc
}

func (r *reducerRegistration) Name() string { return r.name }

// lifecycleRegistration holds a lifecycle reducer binding.
type lifecycleRegistration struct {
	lifecycle  reducer.Lifecycle
	fn         any
	dispatchFn reducer.ReducerFunc
}

// RegisterTable registers a table type. Called during init().
// T must be a struct. Schema is discovered via reflect.
func RegisterTable[T any](name string, access moduledef.TableAccess) TableRegistration {
	var zero T
	t := reflect.TypeOf(zero)
	if t.Kind() == reflect.Ptr {
		t = t.Elem()
	}
	if t.Kind() != reflect.Struct {
		panic(fmt.Sprintf("runtime.RegisterTable: %s must be a struct type, got %v", name, t))
	}

	schema := reflectStructSchema(t)
	plan := buildStructPlan(t)

	reg := tableRegistration{
		name:   name,
		access: access,
		schema: schema,
		goType: t,
		plan:   plan,
		encodeFn: func(v any) []byte {
			return reflectEncode(v)
		},
		decodeFn: func(data []byte) (any, error) {
			return reflectDecode(t, data)
		},
		decodeReaderFn: func(r bsatn.Reader) (any, error) {
			rv := reflect.New(t).Elem()
			if err := reflectDecodeValue(r, rv); err != nil {
				return nil, err
			}
			return rv.Interface(), nil
		},
	}

	registeredTables = append(registeredTables, reg)
	return &registeredTables[len(registeredTables)-1]
}

// RegisterReducer registers a reducer function. Called during init().
// The fn signature must be: func(ctx reducer.ReducerContext, args...) error
// or func(ctx reducer.ReducerContext, args...) for reducers with no error return.
// Args are decoded from BSATN bytes based on the function parameter types.
// Optional paramNames specify names for the reducer parameters (after ReducerContext).
// If not provided, parameters are named "arg_0", "arg_1", etc.
//
// Internally, this builds an optimized dispatch path:
//   - 0-arg reducers: direct type-assertion call (no reflect.Value.Call)
//   - N-arg reducers: pre-compiled fieldDecodeFn per param, pre-allocated storage,
//     zero-copy BSATN reader, reflect.Value.Call with cached arg values
func RegisterReducer(name string, fn any, paramNames ...string) ReducerRegistration {
	fnVal := reflect.ValueOf(fn)
	fnType := fnVal.Type()

	if fnType.Kind() != reflect.Func {
		panic(fmt.Sprintf("runtime.RegisterReducer: %s must be a function, got %v", name, fnType))
	}

	if fnType.NumIn() < 1 {
		panic(fmt.Sprintf("runtime.RegisterReducer: %s must accept at least ReducerContext as first parameter", name))
	}

	numParams := fnType.NumIn() - 1
	if len(paramNames) > 0 && len(paramNames) != numParams {
		panic(fmt.Sprintf("runtime.RegisterReducer: %s has %d params but %d names provided", name, numParams, len(paramNames)))
	}

	// Build parameter ProductType from function args (skip first arg = ReducerContext).
	// Also save reflect.Types for deferred TypeRef resolution in __describe_module__.
	elements := make([]types.ProductTypeElement, 0, numParams)
	paramReflectTypes := make([]reflect.Type, 0, numParams)
	for i := 1; i < fnType.NumIn(); i++ {
		paramType := fnType.In(i)
		paramName := fmt.Sprintf("arg_%d", i-1)
		if len(paramNames) > 0 {
			paramName = paramNames[i-1]
		}
		algType := goTypeToAlgebraic(paramType)
		elements = append(elements, types.ProductTypeElement{
			Name:          paramName,
			AlgebraicType: algType,
		})
		paramReflectTypes = append(paramReflectTypes, paramType)
	}

	paramProductType := types.NewProductType(elements...)

	hasErrorReturn := fnType.NumOut() > 0 && fnType.Out(0).Implements(errorType)

	var dispatchFn reducer.ReducerFunc

	if numParams == 0 {
		// Fast path: 0-arg reducers — direct type assertion, no reflect.Value.Call.
		dispatchFn = buildZeroArgDispatch(fn, fnVal, hasErrorReturn)
	} else {
		// Optimized N-arg path: pre-compiled decoders + pre-allocated storage.
		type fastParam struct {
			decode fieldDecodeFn
			ptr    unsafe.Pointer // pre-allocated storage (reused, single-threaded)
		}
		params := make([]fastParam, numParams)
		callArgs := make([]reflect.Value, fnType.NumIn())

		for i := 0; i < numParams; i++ {
			pt := fnType.In(i + 1)
			params[i].decode = buildParamDecoder(pt)

			// Pre-allocate storage for this parameter. The reflect.Value
			// wrapping it is cached in callArgs and reused across calls.
			// Safe because WASM is single-threaded.
			storage := reflect.New(pt)
			params[i].ptr = unsafe.Pointer(storage.Pointer())
			callArgs[i+1] = storage.Elem()
		}

		dispatchFn = func(ctx reducer.ReducerContext, args []byte) error {
			r := bsatn.NewZeroCopyReader(args)
			callArgs[0] = reflect.ValueOf(ctx)

			for i := range params {
				p := &params[i]
				if err := p.decode(r, p.ptr); err != nil {
					return fmt.Errorf("runtime: failed to decode arg %d for reducer %s: %w", i, name, err)
				}
			}

			results := fnVal.Call(callArgs)

			if hasErrorReturn && len(results) > 0 && !results[0].IsNil() {
				return results[0].Interface().(error)
			}
			return nil
		}
	}

	// Save the explicit names for use in __describe_module__.
	var savedNames []string
	if len(paramNames) > 0 {
		savedNames = make([]string, len(paramNames))
		copy(savedNames, paramNames)
	}

	reg := reducerRegistration{
		name:              name,
		fn:                fn,
		paramType:         paramProductType,
		paramReflectTypes: paramReflectTypes,
		paramNames:        savedNames,
		dispatchFn:        dispatchFn,
	}

	registeredReducers = append(registeredReducers, reg)
	return &registeredReducers[len(registeredReducers)-1]
}

// buildZeroArgDispatch builds a dispatch function for 0-argument reducers.
// Uses direct type assertion to avoid reflect.Value.Call entirely.
func buildZeroArgDispatch(fn any, fnVal reflect.Value, hasErrorReturn bool) reducer.ReducerFunc {
	switch typedFn := fn.(type) {
	case func(reducer.ReducerContext):
		return func(ctx reducer.ReducerContext, _ []byte) error {
			typedFn(ctx)
			return nil
		}
	case func(reducer.ReducerContext) error:
		return func(ctx reducer.ReducerContext, _ []byte) error {
			return typedFn(ctx)
		}
	default:
		// Fallback for unusual 0-arg signatures (e.g., custom ReducerContext subtypes).
		return func(ctx reducer.ReducerContext, _ []byte) error {
			results := fnVal.Call([]reflect.Value{reflect.ValueOf(ctx)})
			if hasErrorReturn && len(results) > 0 && !results[0].IsNil() {
				return results[0].Interface().(error)
			}
			return nil
		}
	}
}

// RegisterLifecycleReducer registers a lifecycle reducer. Called during init().
// The fn should match the appropriate lifecycle signature:
//   - LifecycleInit: func(ctx reducer.ReducerContext)
//   - LifecycleClientConnected: func(ctx reducer.ReducerContext)
//   - LifecycleClientDisconnected: func(ctx reducer.ReducerContext)
func RegisterLifecycleReducer(lifecycle reducer.Lifecycle, fn any) {
	// Fast path: direct type assertion avoids reflect.Value.Call.
	if typedFn, ok := fn.(func(reducer.ReducerContext)); ok {
		registeredLifecycle = append(registeredLifecycle, lifecycleRegistration{
			lifecycle: lifecycle,
			fn:        fn,
			dispatchFn: func(ctx reducer.ReducerContext, _ []byte) error {
				typedFn(ctx)
				return nil
			},
		})
		return
	}

	// Fallback: reflect-based dispatch.
	fnVal := reflect.ValueOf(fn)
	fnType := fnVal.Type()

	if fnType.Kind() != reflect.Func {
		panic(fmt.Sprintf("runtime.RegisterLifecycleReducer: must be a function, got %v", fnType))
	}

	registeredLifecycle = append(registeredLifecycle, lifecycleRegistration{
		lifecycle: lifecycle,
		fn:        fn,
		dispatchFn: func(ctx reducer.ReducerContext, _ []byte) error {
			fnVal.Call([]reflect.Value{reflect.ValueOf(ctx)})
			return nil
		},
	})
}

var errorType = reflect.TypeOf((*error)(nil)).Elem()
