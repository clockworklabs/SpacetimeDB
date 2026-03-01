package runtime

import (
	"fmt"
	"reflect"
	"strings"
	"unicode"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// columnMeta holds metadata extracted from struct tags for a single field.
type columnMeta struct {
	primaryKey bool
	autoInc    bool
	unique     bool
	indexBTree bool
}

// structSchema holds the reflected schema for a struct type.
type structSchema struct {
	productType types.ProductType
	columns     []columnMeta
}

// reflectStructSchema inspects a struct type and returns its ProductType and per-column metadata.
func reflectStructSchema(t reflect.Type) structSchema {
	if t.Kind() == reflect.Ptr {
		t = t.Elem()
	}
	if t.Kind() != reflect.Struct {
		panic(fmt.Sprintf("runtime: RegisterTable requires a struct type, got %v", t))
	}

	elements := make([]types.ProductTypeElement, 0, t.NumField())
	columns := make([]columnMeta, 0, t.NumField())

	for i := 0; i < t.NumField(); i++ {
		field := t.Field(i)
		if !field.IsExported() {
			continue
		}

		name := toSnakeCase(field.Name)
		algType := goTypeToAlgebraic(field.Type)
		elements = append(elements, types.ProductTypeElement{
			Name:          name,
			AlgebraicType: algType,
		})

		meta := parseStructTag(field.Tag.Get("stdb"))
		columns = append(columns, meta)
	}

	return structSchema{
		productType: types.NewProductType(elements...),
		columns:     columns,
	}
}

// parseStructTag parses the "stdb" struct tag into column metadata.
// Format: comma-separated values like "primarykey", "autoinc", "unique", "index=btree".
func parseStructTag(tag string) columnMeta {
	var meta columnMeta
	if tag == "" {
		return meta
	}
	parts := strings.Split(tag, ",")
	for _, part := range parts {
		part = strings.TrimSpace(part)
		switch {
		case part == "primarykey":
			meta.primaryKey = true
		case part == "autoinc":
			meta.autoInc = true
		case part == "unique":
			meta.unique = true
		case part == "index=btree":
			meta.indexBTree = true
		}
	}
	return meta
}

// Special type constructors for the standard SATS representations.
// These match the Rust field-name constants used for structural matching by the codegen.
func algTypeIdentity() types.AlgebraicType {
	return types.AlgTypeProduct(types.NewProductType(
		types.ProductTypeElement{Name: "__identity__", AlgebraicType: types.AlgTypeU256()},
	))
}

func algTypeConnectionId() types.AlgebraicType {
	return types.AlgTypeProduct(types.NewProductType(
		types.ProductTypeElement{Name: "__connection_id__", AlgebraicType: types.AlgTypeU128()},
	))
}

func algTypeTimestamp() types.AlgebraicType {
	return types.AlgTypeProduct(types.NewProductType(
		types.ProductTypeElement{Name: "__timestamp_micros_since_unix_epoch__", AlgebraicType: types.AlgTypeI64()},
	))
}

func algTypeTimeDuration() types.AlgebraicType {
	return types.AlgTypeProduct(types.NewProductType(
		types.ProductTypeElement{Name: "__time_duration_micros__", AlgebraicType: types.AlgTypeI64()},
	))
}

func algTypeScheduleAt() types.AlgebraicType {
	return types.AlgTypeSum(types.NewSumType(
		types.SumTypeVariant{Name: "Interval", AlgebraicType: algTypeTimeDuration()},
		types.SumTypeVariant{Name: "Time", AlgebraicType: algTypeTimestamp()},
	))
}

func algTypeUuid() types.AlgebraicType {
	return types.AlgTypeProduct(types.NewProductType(
		types.ProductTypeElement{Name: "__uuid__", AlgebraicType: types.AlgTypeU128()},
	))
}

// resolveSpecialType checks if a Go type is a known SpacetimeDB special type
// and returns its AlgebraicType. Uses exact type equality to avoid ambiguity
// from overlapping Go interface method sets.
func resolveSpecialType(t reflect.Type) (types.AlgebraicType, bool) {
	switch t {
	case identityType:
		return algTypeIdentity(), true
	case connectionIdType:
		return algTypeConnectionId(), true
	case timestampType:
		return algTypeTimestamp(), true
	case timeDurationType:
		return algTypeTimeDuration(), true
	case uint128Type:
		return types.AlgTypeU128(), true
	case uint256Type:
		return types.AlgTypeU256(), true
	case int128Type:
		return types.AlgTypeI128(), true
	case int256Type:
		return types.AlgTypeI256(), true
	case scheduleAtType:
		return algTypeScheduleAt(), true
	case uuidType:
		return algTypeUuid(), true
	}
	return nil, false
}

// goTypeToAlgebraic maps a Go reflect.Type to a SpacetimeDB AlgebraicType.
func goTypeToAlgebraic(t reflect.Type) types.AlgebraicType {
	// Check for known SpacetimeDB interface types.
	// Use exact type equality (==) instead of Implements because several
	// interfaces have overlapping method sets (e.g., Uint128 satisfies
	// ConnectionId, Uint256 satisfies Identity). Exact match is correct
	// because struct fields use the interface types directly.
	if algType, ok := resolveSpecialType(t); ok {
		return algType
	}

	// Check for registered sum types (interface types with registered variants).
	if t.Kind() == reflect.Interface {
		if info := lookupSumType(t); info != nil {
			return sumTypeAlgebraic(info)
		}
	}

	// Check for registered simple enums (C-style enums backed by integer types).
	if info := lookupSimpleEnum(t); info != nil {
		return simpleEnumAlgebraic(info)
	}

	switch t.Kind() {
	case reflect.Bool:
		return types.AlgTypeBool()
	case reflect.Uint8:
		return types.AlgTypeU8()
	case reflect.Uint16:
		return types.AlgTypeU16()
	case reflect.Uint32:
		return types.AlgTypeU32()
	case reflect.Uint64:
		return types.AlgTypeU64()
	case reflect.Int8:
		return types.AlgTypeI8()
	case reflect.Int16:
		return types.AlgTypeI16()
	case reflect.Int32:
		return types.AlgTypeI32()
	case reflect.Int64:
		return types.AlgTypeI64()
	case reflect.Float32:
		return types.AlgTypeF32()
	case reflect.Float64:
		return types.AlgTypeF64()
	case reflect.String:
		return types.AlgTypeString()
	case reflect.Slice:
		if t.Elem().Kind() == reflect.Uint8 {
			// Check if the element is a registered simple enum — if so, use a
			// typed array rather than treating it as raw []byte.
			if lookupSimpleEnum(t.Elem()) != nil {
				return types.AlgTypeArray(goTypeToAlgebraic(t.Elem()))
			}
			return types.AlgTypeArray(types.AlgTypeU8())
		}
		return types.AlgTypeArray(goTypeToAlgebraic(t.Elem()))
	case reflect.Ptr:
		// *T maps to Option<T> which is a sum type: tag 0 = some(T), tag 1 = none
		inner := goTypeToAlgebraic(t.Elem())
		return types.AlgTypeSum(types.NewSumType(
			types.SumTypeVariant{Name: "some", AlgebraicType: inner},
			types.SumTypeVariant{Name: "none", AlgebraicType: types.AlgTypeProduct(types.NewProductType())},
		))
	case reflect.Struct:
		// Recursive product type for nested structs.
		return types.AlgTypeProduct(reflectStructSchema(t).productType)
	default:
		panic(fmt.Sprintf("runtime: unsupported Go type for AlgebraicType mapping: %v", t))
	}
}

// Interface type references for type-checking SpacetimeDB wrapper types.
var (
	identityType     = reflect.TypeOf((*types.Identity)(nil)).Elem()
	connectionIdType = reflect.TypeOf((*types.ConnectionId)(nil)).Elem()
	timestampType    = reflect.TypeOf((*types.Timestamp)(nil)).Elem()
	timeDurationType = reflect.TypeOf((*types.TimeDuration)(nil)).Elem()
	uint128Type      = reflect.TypeOf((*types.Uint128)(nil)).Elem()
	uint256Type      = reflect.TypeOf((*types.Uint256)(nil)).Elem()
	int128Type       = reflect.TypeOf((*types.Int128)(nil)).Elem()
	int256Type       = reflect.TypeOf((*types.Int256)(nil)).Elem()
	scheduleAtType   = reflect.TypeOf((*types.ScheduleAt)(nil)).Elem()
	uuidType         = reflect.TypeOf((*types.Uuid)(nil)).Elem()
)

// reflectEncode encodes a struct value to BSATN bytes using reflection.
func reflectEncode(v any) []byte {
	w := bsatn.NewWriter(128)
	rv := reflect.ValueOf(v)
	if rv.Kind() == reflect.Ptr {
		rv = rv.Elem()
	}
	reflectEncodeValue(w, rv)
	return w.Bytes()
}

// reflectEncodeValue encodes a single reflect.Value to the BSATN writer.
func reflectEncodeValue(w bsatn.Writer, rv reflect.Value) {
	// Check for bsatn.Serializable interface first.
	if rv.CanInterface() {
		if s, ok := rv.Interface().(bsatn.Serializable); ok {
			s.WriteBsatn(w)
			return
		}
	}
	// Also check pointer-receiver methods if rv is addressable.
	if rv.CanAddr() {
		if s, ok := rv.Addr().Interface().(bsatn.Serializable); ok {
			s.WriteBsatn(w)
			return
		}
	}

	switch rv.Kind() {
	case reflect.Bool:
		w.PutBool(rv.Bool())
	case reflect.Uint8:
		w.PutU8(uint8(rv.Uint()))
	case reflect.Uint16:
		w.PutU16(uint16(rv.Uint()))
	case reflect.Uint32:
		w.PutU32(uint32(rv.Uint()))
	case reflect.Uint64:
		w.PutU64(rv.Uint())
	case reflect.Int8:
		w.PutI8(int8(rv.Int()))
	case reflect.Int16:
		w.PutI16(int16(rv.Int()))
	case reflect.Int32:
		w.PutI32(int32(rv.Int()))
	case reflect.Int64:
		w.PutI64(rv.Int())
	case reflect.Float32:
		w.PutF32(float32(rv.Float()))
	case reflect.Float64:
		w.PutF64(rv.Float())
	case reflect.String:
		w.PutString(rv.String())
	case reflect.Slice:
		if rv.IsNil() {
			w.PutArrayLen(0)
			return
		}
		if rv.Type().Elem().Kind() == reflect.Uint8 && lookupSimpleEnum(rv.Type().Elem()) == nil {
			// []byte: write length + raw bytes (but not simple enum slices).
			b := rv.Bytes()
			w.PutArrayLen(uint32(len(b)))
			w.PutBytes(b)
		} else {
			w.PutArrayLen(uint32(rv.Len()))
			for i := 0; i < rv.Len(); i++ {
				reflectEncodeValue(w, rv.Index(i))
			}
		}
	case reflect.Ptr:
		// Option type: tag 0 = some, tag 1 = none
		if rv.IsNil() {
			w.PutSumTag(1) // None
		} else {
			w.PutSumTag(0) // Some
			reflectEncodeValue(w, rv.Elem())
		}
	case reflect.Interface:
		// Sum type encoding: look up the registered sum type, find variant, write tag + payload.
		if info := lookupSumType(rv.Type()); info != nil {
			if rv.IsNil() {
				panic("runtime: cannot encode nil sum type value")
			}
			elem := rv.Elem()
			concreteType := elem.Type()
			// Handle pointer-stored variants (e.g., &EnumWithPayloadU8{}).
			if concreteType.Kind() == reflect.Ptr {
				concreteType = concreteType.Elem()
				elem = elem.Elem()
			}
			idx, ok := info.typeToIdx[concreteType]
			if !ok {
				panic(fmt.Sprintf("runtime: unknown variant type %v for sum type %v", concreteType, rv.Type()))
			}
			w.PutSumTag(uint8(idx))
			// Encode the payload: each variant struct should have exported fields as the payload.
			if elem.Kind() == reflect.Struct {
				et := elem.Type()
				for i := 0; i < et.NumField(); i++ {
					if !et.Field(i).IsExported() {
						continue
					}
					reflectEncodeValue(w, elem.Field(i))
				}
			} else {
				reflectEncodeValue(w, elem)
			}
		} else {
			panic(fmt.Sprintf("runtime: reflectEncode unsupported interface type: %v", rv.Type()))
		}
	case reflect.Struct:
		rt := rv.Type()
		for i := 0; i < rt.NumField(); i++ {
			if !rt.Field(i).IsExported() {
				continue
			}
			reflectEncodeValue(w, rv.Field(i))
		}
	default:
		panic(fmt.Sprintf("runtime: reflectEncode unsupported kind: %v", rv.Kind()))
	}
}

// reflectDecode decodes BSATN bytes into a struct of the given type using reflection.
func reflectDecode(t reflect.Type, data []byte) (any, error) {
	r := bsatn.NewReader(data)
	rv := reflect.New(t).Elem()
	if err := reflectDecodeValue(r, rv); err != nil {
		return nil, err
	}
	return rv.Interface(), nil
}

// reflectDecodeValue decodes a single value from the BSATN reader into rv.
func reflectDecodeValue(r bsatn.Reader, rv reflect.Value) error {
	rt := rv.Type()

	// Check for known SpacetimeDB interface types stored in fields.
	// Use exact type equality to avoid ambiguity from overlapping method sets.
	switch rt {
	case identityType:
		id, err := types.ReadIdentity(r)
		if err != nil {
			return err
		}
		rv.Set(reflect.ValueOf(id))
		return nil
	case connectionIdType:
		cid, err := types.ReadConnectionId(r)
		if err != nil {
			return err
		}
		rv.Set(reflect.ValueOf(cid))
		return nil
	case timestampType:
		ts, err := types.ReadTimestamp(r)
		if err != nil {
			return err
		}
		rv.Set(reflect.ValueOf(ts))
		return nil
	case timeDurationType:
		td, err := types.ReadTimeDuration(r)
		if err != nil {
			return err
		}
		rv.Set(reflect.ValueOf(td))
		return nil
	case uint128Type:
		v, err := types.ReadUint128(r)
		if err != nil {
			return err
		}
		rv.Set(reflect.ValueOf(v))
		return nil
	case uint256Type:
		v, err := types.ReadUint256(r)
		if err != nil {
			return err
		}
		rv.Set(reflect.ValueOf(v))
		return nil
	case int128Type:
		v, err := types.ReadInt128(r)
		if err != nil {
			return err
		}
		rv.Set(reflect.ValueOf(v))
		return nil
	case int256Type:
		v, err := types.ReadInt256(r)
		if err != nil {
			return err
		}
		rv.Set(reflect.ValueOf(v))
		return nil
	case scheduleAtType:
		v, err := types.ReadScheduleAt(r)
		if err != nil {
			return err
		}
		rv.Set(reflect.ValueOf(v))
		return nil
	case uuidType:
		v, err := types.ReadUuid(r)
		if err != nil {
			return err
		}
		rv.Set(reflect.ValueOf(v))
		return nil
	}

	switch rv.Kind() {
	case reflect.Bool:
		v, err := r.GetBool()
		if err != nil {
			return err
		}
		rv.SetBool(v)
	case reflect.Uint8:
		v, err := r.GetU8()
		if err != nil {
			return err
		}
		rv.SetUint(uint64(v))
	case reflect.Uint16:
		v, err := r.GetU16()
		if err != nil {
			return err
		}
		rv.SetUint(uint64(v))
	case reflect.Uint32:
		v, err := r.GetU32()
		if err != nil {
			return err
		}
		rv.SetUint(uint64(v))
	case reflect.Uint64:
		v, err := r.GetU64()
		if err != nil {
			return err
		}
		rv.SetUint(v)
	case reflect.Int8:
		v, err := r.GetI8()
		if err != nil {
			return err
		}
		rv.SetInt(int64(v))
	case reflect.Int16:
		v, err := r.GetI16()
		if err != nil {
			return err
		}
		rv.SetInt(int64(v))
	case reflect.Int32:
		v, err := r.GetI32()
		if err != nil {
			return err
		}
		rv.SetInt(int64(v))
	case reflect.Int64:
		v, err := r.GetI64()
		if err != nil {
			return err
		}
		rv.SetInt(v)
	case reflect.Float32:
		v, err := r.GetF32()
		if err != nil {
			return err
		}
		rv.SetFloat(float64(v))
	case reflect.Float64:
		v, err := r.GetF64()
		if err != nil {
			return err
		}
		rv.SetFloat(v)
	case reflect.String:
		v, err := r.GetString()
		if err != nil {
			return err
		}
		rv.SetString(v)
	case reflect.Slice:
		arrLen, err := r.GetArrayLen()
		if err != nil {
			return err
		}
		if rt.Elem().Kind() == reflect.Uint8 && lookupSimpleEnum(rt.Elem()) == nil {
			// []byte: read raw bytes (but not simple enum slices).
			b, err := r.GetBytes(int(arrLen))
			if err != nil {
				return err
			}
			rv.SetBytes(b)
		} else {
			slice := reflect.MakeSlice(rt, int(arrLen), int(arrLen))
			for i := 0; i < int(arrLen); i++ {
				if err := reflectDecodeValue(r, slice.Index(i)); err != nil {
					return err
				}
			}
			rv.Set(slice)
		}
	case reflect.Ptr:
		// Option: tag 0 = some, tag 1 = none
		tag, err := r.GetSumTag()
		if err != nil {
			return err
		}
		switch tag {
		case 0: // Some
			elemType := rt.Elem()
			if elemType.Kind() == reflect.Interface {
				// For *InterfaceType (Option of interface), allocate a pointer to
				// the interface and decode the interface value via the existing
				// interface decode paths (known types + registered sum types).
				ptrVal := reflect.New(elemType)
				if err := reflectDecodeValue(r, ptrVal.Elem()); err != nil {
					return err
				}
				rv.Set(ptrVal)
			} else {
				elem := reflect.New(elemType)
				if err := reflectDecodeValue(r, elem.Elem()); err != nil {
					return err
				}
				rv.Set(elem)
			}
		case 1: // None
			rv.Set(reflect.Zero(rt))
		default:
			return fmt.Errorf("runtime: invalid option tag %d", tag)
		}
	case reflect.Interface:
		// Sum type decoding: read tag, create variant struct, decode payload fields.
		if info := lookupSumType(rt); info != nil {
			tag, err := r.GetSumTag()
			if err != nil {
				return err
			}
			if int(tag) >= len(info.variants) {
				return fmt.Errorf("runtime: invalid sum type tag %d for %v (max %d)", tag, rt, len(info.variants)-1)
			}
			variant := info.variants[tag]
			variantVal := reflect.New(variant.Type).Elem()
			// Decode payload fields from the variant struct.
			if variant.Type.Kind() == reflect.Struct {
				for i := 0; i < variant.Type.NumField(); i++ {
					if !variant.Type.Field(i).IsExported() {
						continue
					}
					if err := reflectDecodeValue(r, variantVal.Field(i)); err != nil {
						return err
					}
				}
			}
			rv.Set(variantVal)
		} else {
			return fmt.Errorf("runtime: reflectDecode unsupported interface type: %v", rt)
		}
	case reflect.Struct:
		for i := 0; i < rt.NumField(); i++ {
			if !rt.Field(i).IsExported() {
				continue
			}
			if err := reflectDecodeValue(r, rv.Field(i)); err != nil {
				return err
			}
		}
	default:
		return fmt.Errorf("runtime: reflectDecode unsupported kind: %v", rv.Kind())
	}
	return nil
}

// typeResolver maps Go reflect.Types to SpacetimeDB AlgebraicTypes with TypeRef
// resolution. Struct types and registered sum types are added to the typespace
// and referenced via AlgTypeRef, which is required by the Rust codegen.
type typeResolver struct {
	ts        types.Typespace
	typeMap   map[reflect.Type]types.TypeRef   // goType → TypeRef for structs and sum types
	nameMap   map[reflect.Type]string          // goType → override name (for table types)
	schemaMap map[reflect.Type]structSchema    // cached resolved schemas
	defs      []typeDefEntry                   // accumulated TypeDef entries
}

type typeDefEntry struct {
	name           string
	typeRef        types.TypeRef
	customOrdering bool
}

func newTypeResolver(ts types.Typespace) *typeResolver {
	return &typeResolver{
		ts:        ts,
		typeMap:   make(map[reflect.Type]types.TypeRef),
		nameMap:   make(map[reflect.Type]string),
		schemaMap: make(map[reflect.Type]structSchema),
	}
}

// setTypeName pre-registers a name for a Go type.
// Used for table types whose names come from the registration, not from the struct name.
func (r *typeResolver) setTypeName(t reflect.Type, name string) {
	r.nameMap[t] = name
}

// typeName returns the TypeDef name for a Go type.
// Uses the Go type name (PascalCase) which matches the Rust codegen's expectations.
func (r *typeResolver) typeName(t reflect.Type) string {
	if name, ok := r.nameMap[t]; ok {
		return name
	}
	return t.Name()
}

// typeRefFor returns the TypeRef for a Go type, if it has been resolved.
func (r *typeResolver) typeRefFor(t reflect.Type) (types.TypeRef, bool) {
	ref, ok := r.typeMap[t]
	return ref, ok
}

// resolvedSchema returns the cached resolved schema for a struct type.
func (r *typeResolver) resolvedSchema(t reflect.Type) (structSchema, bool) {
	s, ok := r.schemaMap[t]
	return s, ok
}

// typeDefs returns all accumulated TypeDef entries for building the module definition.
func (r *typeResolver) getTypeDefs() []typeDefEntry {
	return r.defs
}

// setCustomOrdering marks a type's TypeDef as having custom ordering.
// This is needed for table types that have btree indexes.
func (r *typeResolver) setCustomOrdering(t reflect.Type) {
	ref, ok := r.typeMap[t]
	if !ok {
		return
	}
	for i := range r.defs {
		if r.defs[i].typeRef == ref {
			r.defs[i].customOrdering = true
			return
		}
	}
}

// resolveType maps a Go reflect.Type to a SpacetimeDB AlgebraicType.
// Struct types and registered sum types are added to the typespace and
// referenced via AlgTypeRef. Primitive types and special SpacetimeDB types
// are returned inline.
func (r *typeResolver) resolveType(t reflect.Type) types.AlgebraicType {
	// Check for known SpacetimeDB interface types first (inline as "special" types).
	// The codegen recognizes these by structural matching on their field names.
	if algType, ok := resolveSpecialType(t); ok {
		return algType
	}

	// Check for registered sum types (interface types with registered variants).
	if t.Kind() == reflect.Interface {
		if info := lookupSumType(t); info != nil {
			if ref, ok := r.typeMap[t]; ok {
				return types.AlgTypeRef(ref)
			}
			// Reserve a slot, record the TypeRef, then resolve variants.
			// The reserve-then-fill pattern prevents infinite recursion
			// if variants reference types that reference back to this sum type.
			ref := r.ts.Reserve()
			r.typeMap[t] = ref
			algType := r.resolveSumType(info)
			r.ts.Set(ref, algType)
			name := r.typeName(t)
			r.defs = append(r.defs, typeDefEntry{name: name, typeRef: ref, customOrdering: true})
			return types.AlgTypeRef(ref)
		}
	}

	// Check for registered simple enums (C-style enums backed by integer types).
	if info := lookupSimpleEnum(t); info != nil {
		if ref, ok := r.typeMap[t]; ok {
			return types.AlgTypeRef(ref)
		}
		ref := r.ts.Reserve()
		r.typeMap[t] = ref
		algType := simpleEnumAlgebraic(info)
		r.ts.Set(ref, algType)
		name := r.typeName(t)
		r.defs = append(r.defs, typeDefEntry{name: name, typeRef: ref, customOrdering: true})
		return types.AlgTypeRef(ref)
	}

	switch t.Kind() {
	case reflect.Bool:
		return types.AlgTypeBool()
	case reflect.Uint8:
		return types.AlgTypeU8()
	case reflect.Uint16:
		return types.AlgTypeU16()
	case reflect.Uint32:
		return types.AlgTypeU32()
	case reflect.Uint64:
		return types.AlgTypeU64()
	case reflect.Int8:
		return types.AlgTypeI8()
	case reflect.Int16:
		return types.AlgTypeI16()
	case reflect.Int32:
		return types.AlgTypeI32()
	case reflect.Int64:
		return types.AlgTypeI64()
	case reflect.Float32:
		return types.AlgTypeF32()
	case reflect.Float64:
		return types.AlgTypeF64()
	case reflect.String:
		return types.AlgTypeString()
	case reflect.Slice:
		if t.Elem().Kind() == reflect.Uint8 {
			// Check if the element is a registered simple enum — if so, use a
			// typed array rather than treating it as raw []byte.
			if lookupSimpleEnum(t.Elem()) != nil {
				return types.AlgTypeArray(r.resolveType(t.Elem()))
			}
			return types.AlgTypeArray(types.AlgTypeU8())
		}
		return types.AlgTypeArray(r.resolveType(t.Elem()))
	case reflect.Ptr:
		// *T maps to Option<T> which is a sum type: tag 0 = some(T), tag 1 = none
		inner := r.resolveType(t.Elem())
		return types.AlgTypeSum(types.NewSumType(
			types.SumTypeVariant{Name: "some", AlgebraicType: inner},
			types.SumTypeVariant{Name: "none", AlgebraicType: types.AlgTypeProduct(types.NewProductType())},
		))
	case reflect.Struct:
		if ref, ok := r.typeMap[t]; ok {
			return types.AlgTypeRef(ref)
		}
		// Reserve a slot, record the TypeRef, then resolve fields.
		ref := r.ts.Reserve()
		r.typeMap[t] = ref
		schema := r.resolveStructSchemaInner(t)
		r.schemaMap[t] = schema
		r.ts.Set(ref, types.AlgTypeProduct(schema.productType))
		name := r.typeName(t)
		r.defs = append(r.defs, typeDefEntry{name: name, typeRef: ref, customOrdering: true})
		return types.AlgTypeRef(ref)
	default:
		panic(fmt.Sprintf("runtime: unsupported Go type for AlgebraicType mapping: %v", t))
	}
}

// resolveStructSchemaInner resolves a struct type's fields using the resolver.
// This is the inner implementation that actually does the work.
func (r *typeResolver) resolveStructSchemaInner(t reflect.Type) structSchema {
	if t.Kind() == reflect.Ptr {
		t = t.Elem()
	}

	elements := make([]types.ProductTypeElement, 0, t.NumField())
	columns := make([]columnMeta, 0, t.NumField())

	for i := 0; i < t.NumField(); i++ {
		field := t.Field(i)
		if !field.IsExported() {
			continue
		}

		name := toSnakeCase(field.Name)
		algType := r.resolveType(field.Type)
		elements = append(elements, types.ProductTypeElement{
			Name:          name,
			AlgebraicType: algType,
		})

		meta := parseStructTag(field.Tag.Get("stdb"))
		columns = append(columns, meta)
	}

	return structSchema{
		productType: types.NewProductType(elements...),
		columns:     columns,
	}
}

// resolveStructSchema returns the resolved schema for a struct type.
// If the type has already been resolved, returns the cached version.
// Otherwise resolves it (which also adds it to the typespace).
func (r *typeResolver) resolveStructSchema(t reflect.Type) structSchema {
	if schema, ok := r.schemaMap[t]; ok {
		return schema
	}
	// Trigger resolution by calling resolveType, which populates schemaMap.
	r.resolveType(t)
	return r.schemaMap[t]
}

// resolveSumType resolves a registered sum type's variants using the resolver.
func (r *typeResolver) resolveSumType(info *sumTypeInfo) types.AlgebraicType {
	variants := make([]types.SumTypeVariant, len(info.variants))
	for i, v := range info.variants {
		algType := r.resolveVariantPayload(v)
		variants[i] = types.SumTypeVariant{
			Name:          v.Name,
			AlgebraicType: algType,
		}
	}
	return types.AlgTypeSum(types.NewSumType(variants...))
}

// resolveVariantPayload resolves the AlgebraicType for a sum type variant's payload.
// It re-derives the payload type from the variant struct's exported fields
// so that struct types get TypeRefs instead of being inlined.
func (r *typeResolver) resolveVariantPayload(v SumTypeVariantDef) types.AlgebraicType {
	vt := v.Type
	if vt.Kind() != reflect.Struct {
		panic(fmt.Sprintf("runtime: sum type variant %q must be a struct type, got %v", v.Name, vt))
	}

	// Count exported fields.
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
		return r.resolveType(exported[0].Type)
	}

	// Multi-field variant — the payload is a product of all fields.
	elements := make([]types.ProductTypeElement, len(exported))
	for i, f := range exported {
		elements[i] = types.ProductTypeElement{
			Name:          toSnakeCase(f.Name),
			AlgebraicType: r.resolveType(f.Type),
		}
	}
	return types.AlgTypeProduct(types.NewProductType(elements...))
}

// toSnakeCase converts a PascalCase or camelCase string to snake_case.
func toSnakeCase(s string) string {
	var result strings.Builder
	result.Grow(len(s) + 4)

	for i, r := range s {
		if unicode.IsUpper(r) {
			if i > 0 {
				prev := rune(s[i-1])
				if unicode.IsLower(prev) || unicode.IsDigit(prev) {
					result.WriteByte('_')
				} else if unicode.IsUpper(prev) && i+1 < len(s) && unicode.IsLower(rune(s[i+1])) {
					result.WriteByte('_')
				}
			}
			result.WriteRune(unicode.ToLower(r))
		} else {
			result.WriteRune(r)
		}
	}
	return result.String()
}
