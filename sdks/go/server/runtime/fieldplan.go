package runtime

import (
	"fmt"
	"reflect"
	"unsafe"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// fieldOp identifies the BSATN encoding/decoding operation for a single field.
type fieldOp int

const (
	opBool fieldOp = iota
	opU8
	opU16
	opU32
	opU64
	opI8
	opI16
	opI32
	opI64
	opF32
	opF64
	opString
	opByteSlice
	opSlice
	opPtr
	opStruct
	opInterface
	opIdentity
	opConnectionId
	opTimestamp
	opTimeDuration
	opUint128
	opUint256
	opInt128
	opInt256
	opScheduleAt
	opUuid
	opSimpleEnum
)

// fieldDecodeFn decodes a value from a BSATN reader into the memory at ptr.
type fieldDecodeFn func(r bsatn.Reader, ptr unsafe.Pointer) error

// fieldEncodeFn encodes a value from the memory at ptr into a BSATN writer.
type fieldEncodeFn func(w bsatn.Writer, ptr unsafe.Pointer)

// prebuiltField holds pre-compiled decode/encode function pointers for a single field.
type prebuiltField struct {
	offset uintptr
	decode fieldDecodeFn
	encode fieldEncodeFn
}

// fieldInstruction is a pre-computed instruction for encoding/decoding one struct field.
type fieldInstruction struct {
	offset  uintptr     // field offset from struct base pointer
	op      fieldOp     // the operation to perform
	subPlan *structPlan // for opStruct, opSlice of structs, opPtr of struct/special/sum

	// Slice-specific fields
	elemOp   fieldOp      // for opSlice of primitives
	elemSize uintptr      // element size for slice iteration
	elemType reflect.Type // for slice allocation

	// Sum type / interface specific fields
	sumInfo *sumTypeInfo // for opInterface (registered sum types)

	// Pointer-specific fields
	ptrElemOp   fieldOp      // op for the element pointed to (for opPtr)
	ptrElemType reflect.Type // type of element pointed to (for allocation)

	// Simple enum specific
	simpleEnumInfo *simpleEnumInfo
}

// structPlan holds pre-computed field instructions for encoding/decoding a struct type.
type structPlan struct {
	goType   reflect.Type
	fields   []fieldInstruction
	decoders []prebuiltField // pre-built per-field decoders
	encoders []prebuiltField // pre-built per-field encoders
}

// planCache prevents duplicate plan building and handles recursive types.
var planCache = map[reflect.Type]*structPlan{}

// buildStructPlan creates a structPlan for a struct type by walking its fields
// via reflection once. Subsequent encode/decode uses unsafe.Pointer arithmetic
// with pre-computed offsets — zero reflection in the hot path.
func buildStructPlan(t reflect.Type) *structPlan {
	if t.Kind() == reflect.Ptr {
		t = t.Elem()
	}
	if t.Kind() != reflect.Struct {
		panic(fmt.Sprintf("fieldplan: buildStructPlan requires struct, got %v", t))
	}

	// Check cache first.
	if p, ok := planCache[t]; ok {
		return p
	}

	// Reserve the slot before recursing to handle recursive types.
	plan := &structPlan{goType: t}
	planCache[t] = plan

	fields := make([]fieldInstruction, 0, t.NumField())
	for i := 0; i < t.NumField(); i++ {
		sf := t.Field(i)
		if !sf.IsExported() {
			continue
		}
		fi := fieldInstruction{offset: sf.Offset}
		fillFieldInstruction(&fi, sf.Type)
		fields = append(fields, fi)
	}
	plan.fields = fields

	// Build pre-compiled decode/encode function pointers for each field.
	plan.decoders = make([]prebuiltField, len(fields))
	plan.encoders = make([]prebuiltField, len(fields))
	for i := range fields {
		f := &fields[i]
		plan.decoders[i] = prebuiltField{
			offset: f.offset,
			decode: decodeFnForField(f),
		}
		plan.encoders[i] = prebuiltField{
			offset: f.offset,
			encode: encodeFnForField(f),
		}
	}

	return plan
}

// fillFieldInstruction populates a fieldInstruction based on the Go type.
func fillFieldInstruction(fi *fieldInstruction, t reflect.Type) {
	// Check for special SpacetimeDB types first (these are interfaces).
	switch t {
	case identityType:
		fi.op = opIdentity
		return
	case connectionIdType:
		fi.op = opConnectionId
		return
	case timestampType:
		fi.op = opTimestamp
		return
	case timeDurationType:
		fi.op = opTimeDuration
		return
	case uint128Type:
		fi.op = opUint128
		return
	case uint256Type:
		fi.op = opUint256
		return
	case int128Type:
		fi.op = opInt128
		return
	case int256Type:
		fi.op = opInt256
		return
	case scheduleAtType:
		fi.op = opScheduleAt
		return
	case uuidType:
		fi.op = opUuid
		return
	}

	// Check for registered sum types (interface types with registered variants).
	if t.Kind() == reflect.Interface {
		if info := lookupSumType(t); info != nil {
			fi.op = opInterface
			fi.sumInfo = info
			return
		}
	}

	// Check for registered simple enums.
	if info := lookupSimpleEnum(t); info != nil {
		fi.op = opSimpleEnum
		fi.simpleEnumInfo = info
		return
	}

	switch t.Kind() {
	case reflect.Bool:
		fi.op = opBool
	case reflect.Uint8:
		fi.op = opU8
	case reflect.Uint16:
		fi.op = opU16
	case reflect.Uint32:
		fi.op = opU32
	case reflect.Uint64:
		fi.op = opU64
	case reflect.Int8:
		fi.op = opI8
	case reflect.Int16:
		fi.op = opI16
	case reflect.Int32:
		fi.op = opI32
	case reflect.Int64:
		fi.op = opI64
	case reflect.Float32:
		fi.op = opF32
	case reflect.Float64:
		fi.op = opF64
	case reflect.String:
		fi.op = opString
	case reflect.Slice:
		fi.elemType = t.Elem()
		fi.elemSize = t.Elem().Size()
		if t.Elem().Kind() == reflect.Uint8 && lookupSimpleEnum(t.Elem()) == nil {
			// []byte: treated as raw bytes with length prefix.
			fi.op = opByteSlice
		} else {
			fi.op = opSlice
			// Build the element instruction.
			var elemFi fieldInstruction
			fillFieldInstruction(&elemFi, t.Elem())
			fi.elemOp = elemFi.op
			if elemFi.op == opStruct {
				fi.subPlan = elemFi.subPlan
			} else {
				// For non-struct elements, store the full instruction info.
				// We reuse subPlan = nil and elemOp for dispatch.
				fi.subPlan = elemFi.subPlan
				// Copy sum/simple-enum info if needed
				if elemFi.op == opInterface {
					fi.sumInfo = elemFi.sumInfo
				}
				if elemFi.op == opSimpleEnum {
					fi.simpleEnumInfo = elemFi.simpleEnumInfo
				}
			}
		}
	case reflect.Ptr:
		// *T maps to Option<T>
		fi.op = opPtr
		fi.ptrElemType = t.Elem()
		var elemFi fieldInstruction
		fillFieldInstruction(&elemFi, t.Elem())
		fi.ptrElemOp = elemFi.op
		if elemFi.op == opStruct {
			fi.subPlan = elemFi.subPlan
		} else {
			fi.subPlan = elemFi.subPlan
			if elemFi.op == opInterface {
				fi.sumInfo = elemFi.sumInfo
			}
			if elemFi.op == opSimpleEnum {
				fi.simpleEnumInfo = elemFi.simpleEnumInfo
			}
		}
	case reflect.Struct:
		fi.op = opStruct
		fi.subPlan = buildStructPlan(t)
	default:
		panic(fmt.Sprintf("fieldplan: unsupported type %v", t))
	}
}

// sliceHeader mirrors the runtime representation of a Go slice.
type sliceHeader struct {
	Data unsafe.Pointer
	Len  int
	Cap  int
}

// planEncode encodes a struct value from base pointer using pre-built function pointers.
// No reflection or switch dispatch is used in the hot path.
func (p *structPlan) planEncode(w bsatn.Writer, base unsafe.Pointer) {
	for i := range p.encoders {
		e := &p.encoders[i]
		e.encode(w, unsafe.Add(base, e.offset))
	}
}

// encodeField encodes a single field value at ptr using the field instruction.
func encodeField(w bsatn.Writer, f *fieldInstruction, ptr unsafe.Pointer) {
	switch f.op {
	case opBool:
		w.PutBool(*(*bool)(ptr))
	case opU8:
		w.PutU8(*(*uint8)(ptr))
	case opU16:
		w.PutU16(*(*uint16)(ptr))
	case opU32:
		w.PutU32(*(*uint32)(ptr))
	case opU64:
		w.PutU64(*(*uint64)(ptr))
	case opI8:
		w.PutI8(*(*int8)(ptr))
	case opI16:
		w.PutI16(*(*int16)(ptr))
	case opI32:
		w.PutI32(*(*int32)(ptr))
	case opI64:
		w.PutI64(*(*int64)(ptr))
	case opF32:
		w.PutF32(*(*float32)(ptr))
	case opF64:
		w.PutF64(*(*float64)(ptr))
	case opString:
		w.PutString(*(*string)(ptr))
	case opByteSlice:
		sh := (*sliceHeader)(ptr)
		if sh.Data == nil {
			w.PutArrayLen(0)
		} else {
			b := unsafe.Slice((*byte)(sh.Data), sh.Len)
			w.PutArrayLen(uint32(sh.Len))
			w.PutBytes(b)
		}
	case opSlice:
		sh := (*sliceHeader)(ptr)
		if sh.Data == nil {
			w.PutArrayLen(0)
			return
		}
		w.PutArrayLen(uint32(sh.Len))
		encodeSliceElements(w, f, sh)
	case opPtr:
		// Option type: *T
		// The pointer itself is stored at ptr as an unsafe.Pointer-sized value.
		elemPtr := *(*unsafe.Pointer)(ptr)
		if elemPtr == nil {
			w.PutSumTag(1) // None
		} else {
			w.PutSumTag(0) // Some
			encodePtrElement(w, f, elemPtr)
		}
	case opStruct:
		f.subPlan.planEncode(w, ptr)
	case opInterface:
		encodeInterfaceField(w, f, ptr)
	case opIdentity:
		iface := *(*types.Identity)(ptr)
		if iface != nil {
			iface.WriteBsatn(w)
		}
	case opConnectionId:
		iface := *(*types.ConnectionId)(ptr)
		if iface != nil {
			iface.WriteBsatn(w)
		}
	case opTimestamp:
		iface := *(*types.Timestamp)(ptr)
		if iface != nil {
			iface.WriteBsatn(w)
		}
	case opTimeDuration:
		iface := *(*types.TimeDuration)(ptr)
		if iface != nil {
			iface.WriteBsatn(w)
		}
	case opUint128:
		iface := *(*types.Uint128)(ptr)
		if iface != nil {
			iface.WriteBsatn(w)
		}
	case opUint256:
		iface := *(*types.Uint256)(ptr)
		if iface != nil {
			iface.WriteBsatn(w)
		}
	case opInt128:
		iface := *(*types.Int128)(ptr)
		if iface != nil {
			iface.WriteBsatn(w)
		}
	case opInt256:
		iface := *(*types.Int256)(ptr)
		if iface != nil {
			iface.WriteBsatn(w)
		}
	case opScheduleAt:
		iface := *(*types.ScheduleAt)(ptr)
		if iface != nil {
			iface.WriteBsatn(w)
		}
	case opUuid:
		iface := *(*types.Uuid)(ptr)
		if iface != nil {
			iface.WriteBsatn(w)
		}
	case opSimpleEnum:
		// Simple enums are backed by integer types; BSATN encoding is a u8 tag.
		w.PutU8(*(*uint8)(ptr))
	}
}

// encodeSliceElements encodes all elements of a slice.
func encodeSliceElements(w bsatn.Writer, f *fieldInstruction, sh *sliceHeader) {
	switch f.elemOp {
	case opBool:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			w.PutBool(*(*bool)(ep))
		}
	case opU8:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			w.PutU8(*(*uint8)(ep))
		}
	case opU16:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			w.PutU16(*(*uint16)(ep))
		}
	case opU32:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			w.PutU32(*(*uint32)(ep))
		}
	case opU64:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			w.PutU64(*(*uint64)(ep))
		}
	case opI8:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			w.PutI8(*(*int8)(ep))
		}
	case opI16:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			w.PutI16(*(*int16)(ep))
		}
	case opI32:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			w.PutI32(*(*int32)(ep))
		}
	case opI64:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			w.PutI64(*(*int64)(ep))
		}
	case opF32:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			w.PutF32(*(*float32)(ep))
		}
	case opF64:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			w.PutF64(*(*float64)(ep))
		}
	case opString:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			w.PutString(*(*string)(ep))
		}
	case opStruct:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			f.subPlan.planEncode(w, ep)
		}
	case opSimpleEnum:
		for i := 0; i < sh.Len; i++ {
			ep := unsafe.Add(sh.Data, uintptr(i)*f.elemSize)
			w.PutU8(*(*uint8)(ep))
		}
	default:
		// Fall back to reflect for complex element types (interfaces, pointers, etc.)
		sliceVal := reflect.NewAt(reflect.SliceOf(f.elemType), unsafe.Pointer(sh)).Elem()
		for i := 0; i < sh.Len; i++ {
			reflectEncodeValue(w, sliceVal.Index(i))
		}
	}
}

// encodePtrElement encodes the element of a non-nil pointer (Option<T> Some).
func encodePtrElement(w bsatn.Writer, f *fieldInstruction, elemPtr unsafe.Pointer) {
	switch f.ptrElemOp {
	case opBool:
		w.PutBool(*(*bool)(elemPtr))
	case opU8:
		w.PutU8(*(*uint8)(elemPtr))
	case opU16:
		w.PutU16(*(*uint16)(elemPtr))
	case opU32:
		w.PutU32(*(*uint32)(elemPtr))
	case opU64:
		w.PutU64(*(*uint64)(elemPtr))
	case opI8:
		w.PutI8(*(*int8)(elemPtr))
	case opI16:
		w.PutI16(*(*int16)(elemPtr))
	case opI32:
		w.PutI32(*(*int32)(elemPtr))
	case opI64:
		w.PutI64(*(*int64)(elemPtr))
	case opF32:
		w.PutF32(*(*float32)(elemPtr))
	case opF64:
		w.PutF64(*(*float64)(elemPtr))
	case opString:
		w.PutString(*(*string)(elemPtr))
	case opStruct:
		f.subPlan.planEncode(w, elemPtr)
	default:
		// Fall back to reflect for complex pointed-to types
		rv := reflect.NewAt(f.ptrElemType, elemPtr).Elem()
		reflectEncodeValue(w, rv)
	}
}

// encodeInterfaceField encodes a registered sum type interface field.
// This requires one reflect operation per sum field to determine the concrete type.
func encodeInterfaceField(w bsatn.Writer, f *fieldInstruction, ptr unsafe.Pointer) {
	// Read the interface value via reflect (unavoidable for determining concrete type).
	rv := reflect.NewAt(f.sumInfo.ifaceType, ptr).Elem()
	if rv.IsNil() {
		panic("runtime: cannot encode nil sum type value")
	}
	elem := rv.Elem()
	concreteType := elem.Type()
	if concreteType.Kind() == reflect.Ptr {
		concreteType = concreteType.Elem()
		elem = elem.Elem()
	}
	idx, ok := f.sumInfo.typeToIdx[concreteType]
	if !ok {
		panic(fmt.Sprintf("runtime: unknown variant type %v for sum type %v", concreteType, f.sumInfo.ifaceType))
	}
	w.PutSumTag(uint8(idx))
	// Encode the variant's payload fields.
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
}

// planDecode decodes a struct value into base pointer using pre-built function pointers.
// No switch dispatch is used in the hot path.
func (p *structPlan) planDecode(r bsatn.Reader, base unsafe.Pointer) error {
	for i := range p.decoders {
		d := &p.decoders[i]
		if err := d.decode(r, unsafe.Add(base, d.offset)); err != nil {
			return err
		}
	}
	return nil
}

// decodeField decodes a single field value into ptr using the field instruction.
func decodeField(r bsatn.Reader, f *fieldInstruction, ptr unsafe.Pointer) error {
	switch f.op {
	case opBool:
		v, err := r.GetBool()
		if err != nil {
			return err
		}
		*(*bool)(ptr) = v
	case opU8:
		v, err := r.GetU8()
		if err != nil {
			return err
		}
		*(*uint8)(ptr) = v
	case opU16:
		v, err := r.GetU16()
		if err != nil {
			return err
		}
		*(*uint16)(ptr) = v
	case opU32:
		v, err := r.GetU32()
		if err != nil {
			return err
		}
		*(*uint32)(ptr) = v
	case opU64:
		v, err := r.GetU64()
		if err != nil {
			return err
		}
		*(*uint64)(ptr) = v
	case opI8:
		v, err := r.GetI8()
		if err != nil {
			return err
		}
		*(*int8)(ptr) = v
	case opI16:
		v, err := r.GetI16()
		if err != nil {
			return err
		}
		*(*int16)(ptr) = v
	case opI32:
		v, err := r.GetI32()
		if err != nil {
			return err
		}
		*(*int32)(ptr) = v
	case opI64:
		v, err := r.GetI64()
		if err != nil {
			return err
		}
		*(*int64)(ptr) = v
	case opF32:
		v, err := r.GetF32()
		if err != nil {
			return err
		}
		*(*float32)(ptr) = v
	case opF64:
		v, err := r.GetF64()
		if err != nil {
			return err
		}
		*(*float64)(ptr) = v
	case opString:
		v, err := r.GetString()
		if err != nil {
			return err
		}
		*(*string)(ptr) = v
	case opByteSlice:
		arrLen, err := r.GetArrayLen()
		if err != nil {
			return err
		}
		b, err := r.GetBytes(int(arrLen))
		if err != nil {
			return err
		}
		*(*[]byte)(ptr) = b
	case opSlice:
		return decodeSlice(r, f, ptr)
	case opPtr:
		return decodePtr(r, f, ptr)
	case opStruct:
		return f.subPlan.planDecode(r, ptr)
	case opInterface:
		return decodeInterfaceField(r, f, ptr)
	case opIdentity:
		v, err := types.ReadIdentity(r)
		if err != nil {
			return err
		}
		*(*types.Identity)(ptr) = v
	case opConnectionId:
		v, err := types.ReadConnectionId(r)
		if err != nil {
			return err
		}
		*(*types.ConnectionId)(ptr) = v
	case opTimestamp:
		v, err := types.ReadTimestamp(r)
		if err != nil {
			return err
		}
		*(*types.Timestamp)(ptr) = v
	case opTimeDuration:
		v, err := types.ReadTimeDuration(r)
		if err != nil {
			return err
		}
		*(*types.TimeDuration)(ptr) = v
	case opUint128:
		v, err := types.ReadUint128(r)
		if err != nil {
			return err
		}
		*(*types.Uint128)(ptr) = v
	case opUint256:
		v, err := types.ReadUint256(r)
		if err != nil {
			return err
		}
		*(*types.Uint256)(ptr) = v
	case opInt128:
		v, err := types.ReadInt128(r)
		if err != nil {
			return err
		}
		*(*types.Int128)(ptr) = v
	case opInt256:
		v, err := types.ReadInt256(r)
		if err != nil {
			return err
		}
		*(*types.Int256)(ptr) = v
	case opScheduleAt:
		v, err := types.ReadScheduleAt(r)
		if err != nil {
			return err
		}
		*(*types.ScheduleAt)(ptr) = v
	case opUuid:
		v, err := types.ReadUuid(r)
		if err != nil {
			return err
		}
		*(*types.Uuid)(ptr) = v
	case opSimpleEnum:
		v, err := r.GetU8()
		if err != nil {
			return err
		}
		*(*uint8)(ptr) = v
	}
	return nil
}

// decodeSlice decodes a BSATN array into a slice at ptr.
func decodeSlice(r bsatn.Reader, f *fieldInstruction, ptr unsafe.Pointer) error {
	arrLen, err := r.GetArrayLen()
	if err != nil {
		return err
	}
	n := int(arrLen)

	// Use reflect.MakeSlice for GC-safe allocation.
	sliceType := reflect.SliceOf(f.elemType)
	sliceVal := reflect.MakeSlice(sliceType, n, n)

	if n > 0 {
		sliceData := sliceVal.Pointer()
		switch f.elemOp {
		case opBool:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				v, err := r.GetBool()
				if err != nil {
					return err
				}
				*(*bool)(ep) = v
			}
		case opU8:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				v, err := r.GetU8()
				if err != nil {
					return err
				}
				*(*uint8)(ep) = v
			}
		case opU16:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				v, err := r.GetU16()
				if err != nil {
					return err
				}
				*(*uint16)(ep) = v
			}
		case opU32:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				v, err := r.GetU32()
				if err != nil {
					return err
				}
				*(*uint32)(ep) = v
			}
		case opU64:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				v, err := r.GetU64()
				if err != nil {
					return err
				}
				*(*uint64)(ep) = v
			}
		case opI8:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				v, err := r.GetI8()
				if err != nil {
					return err
				}
				*(*int8)(ep) = v
			}
		case opI16:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				v, err := r.GetI16()
				if err != nil {
					return err
				}
				*(*int16)(ep) = v
			}
		case opI32:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				v, err := r.GetI32()
				if err != nil {
					return err
				}
				*(*int32)(ep) = v
			}
		case opI64:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				v, err := r.GetI64()
				if err != nil {
					return err
				}
				*(*int64)(ep) = v
			}
		case opF32:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				v, err := r.GetF32()
				if err != nil {
					return err
				}
				*(*float32)(ep) = v
			}
		case opF64:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				v, err := r.GetF64()
				if err != nil {
					return err
				}
				*(*float64)(ep) = v
			}
		case opString:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				v, err := r.GetString()
				if err != nil {
					return err
				}
				*(*string)(ep) = v
			}
		case opStruct:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				if err := f.subPlan.planDecode(r, ep); err != nil {
					return err
				}
			}
		case opSimpleEnum:
			for i := 0; i < n; i++ {
				ep := unsafe.Add(unsafe.Pointer(sliceData), uintptr(i)*f.elemSize)
				v, err := r.GetU8()
				if err != nil {
					return err
				}
				*(*uint8)(ep) = v
			}
		default:
			// Fall back to reflect for complex element types.
			for i := 0; i < n; i++ {
				if err := reflectDecodeValue(r, sliceVal.Index(i)); err != nil {
					return err
				}
			}
		}
	}

	// Write the slice header into the target location.
	reflect.NewAt(sliceType, ptr).Elem().Set(sliceVal)
	return nil
}

// decodePtr decodes a BSATN Option (sum type with tag 0=Some, 1=None) into a pointer field at ptr.
func decodePtr(r bsatn.Reader, f *fieldInstruction, ptr unsafe.Pointer) error {
	tag, err := r.GetSumTag()
	if err != nil {
		return err
	}
	switch tag {
	case 0: // Some
		// Allocate the pointed-to value via reflect for GC safety.
		ptrVal := reflect.New(f.ptrElemType)
		elemPtr := ptrVal.Pointer()

		switch f.ptrElemOp {
		case opBool:
			v, err := r.GetBool()
			if err != nil {
				return err
			}
			*(*bool)(unsafe.Pointer(elemPtr)) = v
		case opU8:
			v, err := r.GetU8()
			if err != nil {
				return err
			}
			*(*uint8)(unsafe.Pointer(elemPtr)) = v
		case opU16:
			v, err := r.GetU16()
			if err != nil {
				return err
			}
			*(*uint16)(unsafe.Pointer(elemPtr)) = v
		case opU32:
			v, err := r.GetU32()
			if err != nil {
				return err
			}
			*(*uint32)(unsafe.Pointer(elemPtr)) = v
		case opU64:
			v, err := r.GetU64()
			if err != nil {
				return err
			}
			*(*uint64)(unsafe.Pointer(elemPtr)) = v
		case opI8:
			v, err := r.GetI8()
			if err != nil {
				return err
			}
			*(*int8)(unsafe.Pointer(elemPtr)) = v
		case opI16:
			v, err := r.GetI16()
			if err != nil {
				return err
			}
			*(*int16)(unsafe.Pointer(elemPtr)) = v
		case opI32:
			v, err := r.GetI32()
			if err != nil {
				return err
			}
			*(*int32)(unsafe.Pointer(elemPtr)) = v
		case opI64:
			v, err := r.GetI64()
			if err != nil {
				return err
			}
			*(*int64)(unsafe.Pointer(elemPtr)) = v
		case opF32:
			v, err := r.GetF32()
			if err != nil {
				return err
			}
			*(*float32)(unsafe.Pointer(elemPtr)) = v
		case opF64:
			v, err := r.GetF64()
			if err != nil {
				return err
			}
			*(*float64)(unsafe.Pointer(elemPtr)) = v
		case opString:
			v, err := r.GetString()
			if err != nil {
				return err
			}
			*(*string)(unsafe.Pointer(elemPtr)) = v
		case opStruct:
			if err := f.subPlan.planDecode(r, unsafe.Pointer(elemPtr)); err != nil {
				return err
			}
		default:
			// Fall back to reflect for complex element types.
			if err := reflectDecodeValue(r, ptrVal.Elem()); err != nil {
				return err
			}
		}

		// Set the pointer at the field location.
		reflect.NewAt(reflect.PointerTo(f.ptrElemType), ptr).Elem().Set(ptrVal)
	case 1: // None
		// Set the pointer to nil.
		*(*unsafe.Pointer)(ptr) = nil
	default:
		return fmt.Errorf("fieldplan: invalid option tag %d", tag)
	}
	return nil
}

// decodeInterfaceField decodes a registered sum type interface from a BSATN reader.
func decodeInterfaceField(r bsatn.Reader, f *fieldInstruction, ptr unsafe.Pointer) error {
	tag, err := r.GetSumTag()
	if err != nil {
		return err
	}
	if int(tag) >= len(f.sumInfo.variants) {
		return fmt.Errorf("fieldplan: invalid sum type tag %d for %v (max %d)", tag, f.sumInfo.ifaceType, len(f.sumInfo.variants)-1)
	}
	variant := f.sumInfo.variants[tag]
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
	reflect.NewAt(f.sumInfo.ifaceType, ptr).Elem().Set(variantVal)
	return nil
}

// --- Top-level decode functions (no closure, no switch dispatch) ---

func decodeBoolField(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := r.GetBool()
	if err != nil {
		return err
	}
	*(*bool)(ptr) = v
	return nil
}

func decodeU8Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := r.GetU8()
	if err != nil {
		return err
	}
	*(*uint8)(ptr) = v
	return nil
}

func decodeU16Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := r.GetU16()
	if err != nil {
		return err
	}
	*(*uint16)(ptr) = v
	return nil
}

func decodeU32Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := r.GetU32()
	if err != nil {
		return err
	}
	*(*uint32)(ptr) = v
	return nil
}

func decodeU64Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := r.GetU64()
	if err != nil {
		return err
	}
	*(*uint64)(ptr) = v
	return nil
}

func decodeI8Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := r.GetI8()
	if err != nil {
		return err
	}
	*(*int8)(ptr) = v
	return nil
}

func decodeI16Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := r.GetI16()
	if err != nil {
		return err
	}
	*(*int16)(ptr) = v
	return nil
}

func decodeI32Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := r.GetI32()
	if err != nil {
		return err
	}
	*(*int32)(ptr) = v
	return nil
}

func decodeI64Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := r.GetI64()
	if err != nil {
		return err
	}
	*(*int64)(ptr) = v
	return nil
}

func decodeF32Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := r.GetF32()
	if err != nil {
		return err
	}
	*(*float32)(ptr) = v
	return nil
}

func decodeF64Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := r.GetF64()
	if err != nil {
		return err
	}
	*(*float64)(ptr) = v
	return nil
}

func decodeStringField(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := r.GetString()
	if err != nil {
		return err
	}
	*(*string)(ptr) = v
	return nil
}

func decodeByteSliceField(r bsatn.Reader, ptr unsafe.Pointer) error {
	arrLen, err := r.GetArrayLen()
	if err != nil {
		return err
	}
	b, err := r.GetBytes(int(arrLen))
	if err != nil {
		return err
	}
	*(*[]byte)(ptr) = b
	return nil
}

func decodeIdentityField(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := types.ReadIdentity(r)
	if err != nil {
		return err
	}
	*(*types.Identity)(ptr) = v
	return nil
}

func decodeConnectionIdField(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := types.ReadConnectionId(r)
	if err != nil {
		return err
	}
	*(*types.ConnectionId)(ptr) = v
	return nil
}

func decodeTimestampField(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := types.ReadTimestamp(r)
	if err != nil {
		return err
	}
	*(*types.Timestamp)(ptr) = v
	return nil
}

func decodeTimeDurationField(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := types.ReadTimeDuration(r)
	if err != nil {
		return err
	}
	*(*types.TimeDuration)(ptr) = v
	return nil
}

func decodeUint128Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := types.ReadUint128(r)
	if err != nil {
		return err
	}
	*(*types.Uint128)(ptr) = v
	return nil
}

func decodeUint256Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := types.ReadUint256(r)
	if err != nil {
		return err
	}
	*(*types.Uint256)(ptr) = v
	return nil
}

func decodeInt128Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := types.ReadInt128(r)
	if err != nil {
		return err
	}
	*(*types.Int128)(ptr) = v
	return nil
}

func decodeInt256Field(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := types.ReadInt256(r)
	if err != nil {
		return err
	}
	*(*types.Int256)(ptr) = v
	return nil
}

func decodeScheduleAtField(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := types.ReadScheduleAt(r)
	if err != nil {
		return err
	}
	*(*types.ScheduleAt)(ptr) = v
	return nil
}

func decodeUuidField(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := types.ReadUuid(r)
	if err != nil {
		return err
	}
	*(*types.Uuid)(ptr) = v
	return nil
}

func decodeSimpleEnumField(r bsatn.Reader, ptr unsafe.Pointer) error {
	v, err := r.GetU8()
	if err != nil {
		return err
	}
	*(*uint8)(ptr) = v
	return nil
}

// --- Top-level encode functions (no closure, no switch dispatch) ---

func encodeBoolField(w bsatn.Writer, ptr unsafe.Pointer) { w.PutBool(*(*bool)(ptr)) }
func encodeU8Field(w bsatn.Writer, ptr unsafe.Pointer)   { w.PutU8(*(*uint8)(ptr)) }
func encodeU16Field(w bsatn.Writer, ptr unsafe.Pointer)  { w.PutU16(*(*uint16)(ptr)) }
func encodeU32Field(w bsatn.Writer, ptr unsafe.Pointer)  { w.PutU32(*(*uint32)(ptr)) }
func encodeU64Field(w bsatn.Writer, ptr unsafe.Pointer)  { w.PutU64(*(*uint64)(ptr)) }
func encodeI8Field(w bsatn.Writer, ptr unsafe.Pointer)   { w.PutI8(*(*int8)(ptr)) }
func encodeI16Field(w bsatn.Writer, ptr unsafe.Pointer)  { w.PutI16(*(*int16)(ptr)) }
func encodeI32Field(w bsatn.Writer, ptr unsafe.Pointer)  { w.PutI32(*(*int32)(ptr)) }
func encodeI64Field(w bsatn.Writer, ptr unsafe.Pointer)  { w.PutI64(*(*int64)(ptr)) }
func encodeF32Field(w bsatn.Writer, ptr unsafe.Pointer)  { w.PutF32(*(*float32)(ptr)) }
func encodeF64Field(w bsatn.Writer, ptr unsafe.Pointer)  { w.PutF64(*(*float64)(ptr)) }
func encodeStringField(w bsatn.Writer, ptr unsafe.Pointer) {
	w.PutString(*(*string)(ptr))
}

func encodeByteSliceField(w bsatn.Writer, ptr unsafe.Pointer) {
	sh := (*sliceHeader)(ptr)
	if sh.Data == nil {
		w.PutArrayLen(0)
	} else {
		b := unsafe.Slice((*byte)(sh.Data), sh.Len)
		w.PutArrayLen(uint32(sh.Len))
		w.PutBytes(b)
	}
}

func encodeIdentityField(w bsatn.Writer, ptr unsafe.Pointer) {
	iface := *(*types.Identity)(ptr)
	if iface != nil {
		iface.WriteBsatn(w)
	}
}

func encodeConnectionIdField(w bsatn.Writer, ptr unsafe.Pointer) {
	iface := *(*types.ConnectionId)(ptr)
	if iface != nil {
		iface.WriteBsatn(w)
	}
}

func encodeTimestampField(w bsatn.Writer, ptr unsafe.Pointer) {
	iface := *(*types.Timestamp)(ptr)
	if iface != nil {
		iface.WriteBsatn(w)
	}
}

func encodeTimeDurationField(w bsatn.Writer, ptr unsafe.Pointer) {
	iface := *(*types.TimeDuration)(ptr)
	if iface != nil {
		iface.WriteBsatn(w)
	}
}

func encodeUint128Field(w bsatn.Writer, ptr unsafe.Pointer) {
	iface := *(*types.Uint128)(ptr)
	if iface != nil {
		iface.WriteBsatn(w)
	}
}

func encodeUint256Field(w bsatn.Writer, ptr unsafe.Pointer) {
	iface := *(*types.Uint256)(ptr)
	if iface != nil {
		iface.WriteBsatn(w)
	}
}

func encodeInt128Field(w bsatn.Writer, ptr unsafe.Pointer) {
	iface := *(*types.Int128)(ptr)
	if iface != nil {
		iface.WriteBsatn(w)
	}
}

func encodeInt256Field(w bsatn.Writer, ptr unsafe.Pointer) {
	iface := *(*types.Int256)(ptr)
	if iface != nil {
		iface.WriteBsatn(w)
	}
}

func encodeScheduleAtField(w bsatn.Writer, ptr unsafe.Pointer) {
	iface := *(*types.ScheduleAt)(ptr)
	if iface != nil {
		iface.WriteBsatn(w)
	}
}

func encodeUuidField(w bsatn.Writer, ptr unsafe.Pointer) {
	iface := *(*types.Uuid)(ptr)
	if iface != nil {
		iface.WriteBsatn(w)
	}
}

func encodeSimpleEnumField(w bsatn.Writer, ptr unsafe.Pointer) {
	w.PutU8(*(*uint8)(ptr))
}

// --- decodeFnForField: returns a top-level decode func for primitives/specials,
// or a closure for complex types (slice, ptr, struct, interface). ---

func decodeFnForField(f *fieldInstruction) fieldDecodeFn {
	switch f.op {
	case opBool:
		return decodeBoolField
	case opU8:
		return decodeU8Field
	case opU16:
		return decodeU16Field
	case opU32:
		return decodeU32Field
	case opU64:
		return decodeU64Field
	case opI8:
		return decodeI8Field
	case opI16:
		return decodeI16Field
	case opI32:
		return decodeI32Field
	case opI64:
		return decodeI64Field
	case opF32:
		return decodeF32Field
	case opF64:
		return decodeF64Field
	case opString:
		return decodeStringField
	case opByteSlice:
		return decodeByteSliceField
	case opIdentity:
		return decodeIdentityField
	case opConnectionId:
		return decodeConnectionIdField
	case opTimestamp:
		return decodeTimestampField
	case opTimeDuration:
		return decodeTimeDurationField
	case opUint128:
		return decodeUint128Field
	case opUint256:
		return decodeUint256Field
	case opInt128:
		return decodeInt128Field
	case opInt256:
		return decodeInt256Field
	case opScheduleAt:
		return decodeScheduleAtField
	case opUuid:
		return decodeUuidField
	case opSimpleEnum:
		return decodeSimpleEnumField

	// Complex types require closures that capture field metadata.
	case opSlice:
		fi := f // capture pointer
		return func(r bsatn.Reader, ptr unsafe.Pointer) error {
			return decodeSlice(r, fi, ptr)
		}
	case opPtr:
		fi := f
		return func(r bsatn.Reader, ptr unsafe.Pointer) error {
			return decodePtr(r, fi, ptr)
		}
	case opStruct:
		// Capture subPlan pointer — called at invocation time, so recursive types work.
		subPlan := f.subPlan
		return func(r bsatn.Reader, ptr unsafe.Pointer) error {
			return subPlan.planDecode(r, ptr)
		}
	case opInterface:
		fi := f
		return func(r bsatn.Reader, ptr unsafe.Pointer) error {
			return decodeInterfaceField(r, fi, ptr)
		}
	default:
		panic(fmt.Sprintf("fieldplan: unsupported op %d in decodeFnForField", f.op))
	}
}

// --- encodeFnForField: returns a top-level encode func for primitives/specials,
// or a closure for complex types (slice, ptr, struct, interface). ---

func encodeFnForField(f *fieldInstruction) fieldEncodeFn {
	switch f.op {
	case opBool:
		return encodeBoolField
	case opU8:
		return encodeU8Field
	case opU16:
		return encodeU16Field
	case opU32:
		return encodeU32Field
	case opU64:
		return encodeU64Field
	case opI8:
		return encodeI8Field
	case opI16:
		return encodeI16Field
	case opI32:
		return encodeI32Field
	case opI64:
		return encodeI64Field
	case opF32:
		return encodeF32Field
	case opF64:
		return encodeF64Field
	case opString:
		return encodeStringField
	case opByteSlice:
		return encodeByteSliceField
	case opIdentity:
		return encodeIdentityField
	case opConnectionId:
		return encodeConnectionIdField
	case opTimestamp:
		return encodeTimestampField
	case opTimeDuration:
		return encodeTimeDurationField
	case opUint128:
		return encodeUint128Field
	case opUint256:
		return encodeUint256Field
	case opInt128:
		return encodeInt128Field
	case opInt256:
		return encodeInt256Field
	case opScheduleAt:
		return encodeScheduleAtField
	case opUuid:
		return encodeUuidField
	case opSimpleEnum:
		return encodeSimpleEnumField

	case opSlice:
		fi := f
		return func(w bsatn.Writer, ptr unsafe.Pointer) {
			sh := (*sliceHeader)(ptr)
			if sh.Data == nil {
				w.PutArrayLen(0)
				return
			}
			w.PutArrayLen(uint32(sh.Len))
			encodeSliceElements(w, fi, sh)
		}
	case opPtr:
		fi := f
		return func(w bsatn.Writer, ptr unsafe.Pointer) {
			elemPtr := *(*unsafe.Pointer)(ptr)
			if elemPtr == nil {
				w.PutSumTag(1) // None
			} else {
				w.PutSumTag(0) // Some
				encodePtrElement(w, fi, elemPtr)
			}
		}
	case opStruct:
		subPlan := f.subPlan
		return func(w bsatn.Writer, ptr unsafe.Pointer) {
			subPlan.planEncode(w, ptr)
		}
	case opInterface:
		fi := f
		return func(w bsatn.Writer, ptr unsafe.Pointer) {
			encodeInterfaceField(w, fi, ptr)
		}
	default:
		panic(fmt.Sprintf("fieldplan: unsupported op %d in encodeFnForField", f.op))
	}
}

// buildParamDecoder builds a fieldDecodeFn for a single reducer parameter type.
// Used by RegisterReducer to pre-compile decode functions for each parameter.
func buildParamDecoder(pt reflect.Type) fieldDecodeFn {
	var fi fieldInstruction
	fillFieldInstruction(&fi, pt)
	return decodeFnForField(&fi)
}
