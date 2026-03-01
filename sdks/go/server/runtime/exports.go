//go:build wasip1

package runtime

import (
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/moduledef"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/reducer"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/sys"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

const (
	callReducerSuccess int32 = 0
	callReducerErr     int32 = 1 // HOST_CALL_FAILURE: user error or panic
)

// __describe_module__ is called by the host to get the module definition.
//
//go:wasmexport __describe_module__
func wasmDescribeModule(descriptionSink uint32) {
	ts := types.NewTypespace()
	builder := moduledef.NewModuleDefBuilder()
	resolver := newTypeResolver(ts)

	// Pre-register table type names using PascalCase Go struct names,
	// which matches the Rust codegen's expectations for TypeDef source names.
	for i := range registeredTables {
		resolver.setTypeName(registeredTables[i].goType, registeredTables[i].goType.Name())
	}

	// Add all registered table types to the typespace and build table defs.
	// The resolver recursively resolves field types, adding nested struct/sum
	// types to the typespace as needed.
	for i := range registeredTables {
		reg := &registeredTables[i]

		// Resolve the table's struct type (adds to typespace with TypeRef).
		resolver.resolveType(reg.goType)
		ref, ok := resolver.typeRefFor(reg.goType)
		if !ok {
			panic(fmt.Sprintf("runtime: table type %s not resolved to TypeRef", reg.name))
		}
		reg.typeRef = ref

		// Get the resolved schema for column metadata.
		schema := resolver.resolveStructSchema(reg.goType)

		tblBuilder := moduledef.NewTableDefBuilder(reg.name).
			WithProductTypeRef(ref).
			WithTableAccess(reg.access)

		// Process column metadata for indexes, constraints, sequences, primary key.
		elements := schema.productType.Elements()
		var primaryKeyCols []uint16
		hasBTreeIndex := false
		for colIdx, meta := range schema.columns {
			col := uint16(colIdx)
			if meta.primaryKey {
				primaryKeyCols = append(primaryKeyCols, col)
			}
			if meta.autoInc {
				tblBuilder = tblBuilder.WithSequence(
					moduledef.NewSequenceDefBuilder(nil, col).Build(),
				)
			}
			if meta.unique {
				tblBuilder = tblBuilder.WithConstraint(
					moduledef.NewUniqueConstraint(nil, col),
				)
				idxName := fmt.Sprintf("%s_%s_idx_btree", reg.name, elements[colIdx].Name)
				tblBuilder = tblBuilder.WithIndex(
					moduledef.NewBTreeIndexDef(&idxName, col).Build(),
				)
				hasBTreeIndex = true
			}
			if meta.indexBTree {
				idxName := fmt.Sprintf("%s_%s_idx_btree", reg.name, elements[colIdx].Name)
				tblBuilder = tblBuilder.WithIndex(
					moduledef.NewBTreeIndexDef(&idxName, col).Build(),
				)
				hasBTreeIndex = true
			}
		}
		if len(primaryKeyCols) > 0 {
			tblBuilder = tblBuilder.WithPrimaryKey(primaryKeyCols...)
			// Primary key columns implicitly need unique constraints.
			for _, pkCol := range primaryKeyCols {
				tblBuilder = tblBuilder.WithConstraint(
					moduledef.NewUniqueConstraint(nil, pkCol),
				)
				idxName := fmt.Sprintf("%s_%s_idx_btree", reg.name, elements[pkCol].Name)
				tblBuilder = tblBuilder.WithIndex(
					moduledef.NewBTreeIndexDef(&idxName, pkCol).Build(),
				)
				hasBTreeIndex = true
			}
		}

		// Mark table types with btree indexes as needing custom ordering.
		if hasBTreeIndex {
			resolver.setCustomOrdering(reg.goType)
		}

		builder = builder.AddTable(tblBuilder.Build())
	}

	// Add all registered reducers with resolved param types.
	for i := range registeredReducers {
		reg := &registeredReducers[i]

		// Rebuild paramType using the resolver so struct/sum types use TypeRefs.
		elements := make([]types.ProductTypeElement, len(reg.paramReflectTypes))
		for j, pt := range reg.paramReflectTypes {
			paramName := fmt.Sprintf("arg_%d", j)
			if len(reg.paramNames) > j {
				paramName = reg.paramNames[j]
			}
			elements[j] = types.ProductTypeElement{
				Name:          paramName,
				AlgebraicType: resolver.resolveType(pt),
			}
		}
		paramType := types.NewProductType(elements...)

		rBuilder := moduledef.NewReducerDefBuilder(reg.name).
			WithParams(paramType).
			WithVisibility(moduledef.FunctionVisibilityClientCallable).
			WithErrReturnType(types.AlgTypeString())
		builder = builder.AddReducer(rBuilder.Build())
	}

	// Add all lifecycle reducers.
	// They are also added to the reducer list so IDs are contiguous.
	for i := range registeredLifecycle {
		reg := &registeredLifecycle[i]
		lcName := reg.lifecycle.String()

		// Add as a regular reducer with private visibility.
		rBuilder := moduledef.NewReducerDefBuilder(lcName).
			WithParams(types.NewProductType()).
			WithVisibility(moduledef.FunctionVisibilityPrivate).
			WithErrReturnType(types.AlgTypeString())
		builder = builder.AddReducer(rBuilder.Build())

		// Also register as lifecycle reducer.
		var mdLifecycle moduledef.Lifecycle
		switch reg.lifecycle {
		case reducer.LifecycleInit:
			mdLifecycle = moduledef.LifecycleInit
		case reducer.LifecycleClientConnected:
			mdLifecycle = moduledef.LifecycleOnConnect
		case reducer.LifecycleClientDisconnected:
			mdLifecycle = moduledef.LifecycleOnDisconnect
		}
		builder = builder.AddLifecycleReducer(
			moduledef.NewLifecycleReducerDef(mdLifecycle, lcName),
		)
	}

	// Add all TypeDefs from the resolver (covers table types, nested struct types, sum types).
	for _, td := range resolver.getTypeDefs() {
		builder = builder.AddTypeDef(
			moduledef.NewTypeDefBuilder(nil, td.name, td.typeRef).
				WithCustomOrdering(td.customOrdering).Build(),
		)
	}

	// Add all registered RLS (client visibility) filters.
	for _, sql := range registeredRLS {
		builder = builder.AddRowLevelSecurity(sql)
	}

	builder = builder.SetTypespace(ts)
	moduleDef := builder.Build()

	data := bsatn.Encode(moduleDef)
	_ = sys.WriteBytesToSink(descriptionSink, data)
}

// __call_reducer__ is called by the host to execute a reducer.
//
//go:wasmexport __call_reducer__
func wasmCallReducer(id uint32, sender0, sender1, sender2, sender3, connId0, connId1, timestamp uint64, args uint32, errSink uint32) (retCode int32) {
	// Recover from panics in reducer functions (e.g. Insert/Delete/UpdateBy/DeleteBy
	// panic on host errors, matching Rust SDK behavior where these operations panic).
	defer func() {
		if r := recover(); r != nil {
			writeError(errSink, fmt.Sprintf("%v", r))
			retCode = callReducerErr
		}
	}()

	identity := types.NewIdentityFromU64s(sender0, sender1, sender2, sender3)
	connId := types.NewConnectionIdFromU64s(connId0, connId1)
	ts := types.NewTimestamp(int64(timestamp))
	ctx := reducer.NewReducerContext(identity, connId, ts)

	argsData, err := sys.ReadBytesSource(args)
	if err != nil {
		writeError(errSink, fmt.Sprintf("failed to read args: %v", err))
		return callReducerErr
	}

	// The id indexes into the combined list: first registered reducers, then lifecycle reducers.
	totalReducers := uint32(len(registeredReducers))
	totalAll := totalReducers + uint32(len(registeredLifecycle))

	if id >= totalAll {
		writeError(errSink, fmt.Sprintf("reducer id %d out of range (total: %d)", id, totalAll))
		return callReducerErr
	}

	var dispatchFn reducer.ReducerFunc
	if id < totalReducers {
		dispatchFn = registeredReducers[id].dispatchFn
	} else {
		lcIdx := id - totalReducers
		dispatchFn = registeredLifecycle[lcIdx].dispatchFn
	}

	if err := dispatchFn(ctx, argsData); err != nil {
		writeError(errSink, err.Error())
		return callReducerErr
	}

	return callReducerSuccess
}

// __preinit__10_register is called before describe_module.
// Go's init() functions have already run by this point.
//
//go:wasmexport __preinit__10_register
func wasmPreinit() {
	// No-op: Go init() functions run before any wasmexport is called.
}

// writeError writes an error message to the given sink.
func writeError(sink uint32, msg string) {
	_ = sys.WriteBytesToSink(sink, []byte(msg))
}
