package bsatn

import (
	"bytes"
	"fmt"
	"reflect"
	"sort"
	"strings"
)

// Marshal converts a Go value into BSATN bytes.
func Marshal(v interface{}) ([]byte, error) {
	var buf bytes.Buffer
	w := NewWriter(&buf)
	marshalRecursive(w, v)
	if w.Error() != nil {
		return nil, w.Error()
	}
	return buf.Bytes(), nil
}

// marshalRecursive is the actual marshaling logic that uses the Writer.
func marshalRecursive(w *Writer, v interface{}) {
	if w.Error() != nil {
		return
	}

	// Handle IStructuralReadWrite first, as it provides its own BSATN methods.
	if srw, ok := v.(IStructuralReadWrite); ok {
		err := srw.WriteBSATN(w)
		if err != nil {
			w.recordError(err)
		}
		return
	}

	// Handle specific known types before general reflection.
	switch val := v.(type) {
	case nil:
		w.WriteNilOption()
	case AlgebraicType:
		MarshalAlgebraicType(w, val)
	case Variant:
		w.WriteEnumHeader(val.Index)
		if val.Value != nil {
			marshalRecursive(w, val.Value)
		}
	case Int128: // New case
		w.WriteI128Bytes(val.Bytes)
	case Uint128: // New case
		w.WriteU128Bytes(val.Bytes)
	case bool:
		w.WriteBool(val)
	case uint8:
		w.WriteUint8(val)
	case int8:
		w.WriteInt8(val)
	case uint16:
		w.WriteUint16(val)
	case int16:
		w.WriteInt16(val)
	case uint32:
		w.WriteUint32(val)
	case int32:
		w.WriteInt32(val)
	case uint64:
		w.WriteUint64(val)
	case int64:
		w.WriteInt64(val)
	case float32:
		w.WriteFloat32(val)
	case float64:
		w.WriteFloat64(val)
	case string:
		w.WriteString(val)
	case []byte:
		w.WriteBytes(val)
	default:
		// Fallback to reflection for Ptr, other Slices, Arrays, Maps, Structs
		rv := reflect.ValueOf(v)
		switch rv.Kind() {
		case reflect.Ptr:
			if rv.IsNil() {
				w.WriteNilOption()
			} else {
				marshalRecursive(w, rv.Elem().Interface())
			}
		case reflect.Slice:
			w.WriteListHeader(rv.Len())
			for i := 0; i < rv.Len(); i++ {
				marshalRecursive(w, rv.Index(i).Interface())
				if w.Error() != nil {
					return
				}
			}
		case reflect.Array:
			if rv.Type().Elem().Kind() == reflect.Uint8 {
				b := make([]byte, rv.Len())
				for i := 0; i < rv.Len(); i++ {
					b[i] = byte(rv.Index(i).Uint())
				}
				w.WriteBytes(b)
			} else {
				w.WriteArrayHeader(rv.Len())
				for i := 0; i < rv.Len(); i++ {
					marshalRecursive(w, rv.Index(i).Interface())
					if w.Error() != nil {
						return
					}
				}
			}
		case reflect.Map:
			if rv.Type().Key().Kind() != reflect.String {
				w.recordError(fmt.Errorf("bsatn: map key type must be string, got %s", rv.Type().Key().Kind()))
				return
			}
			mapKeys := rv.MapKeys()
			keys := make([]string, len(mapKeys))
			for i, k := range mapKeys {
				keys[i] = k.String()
			}
			sort.Strings(keys)
			w.WriteStructHeader(len(keys))
			for _, k := range keys {
				if w.Error() != nil {
					return
				}
				w.WriteFieldName(k)
				if w.Error() != nil {
					return
				}
				mapVal := rv.MapIndex(reflect.ValueOf(k))
				marshalRecursive(w, mapVal.Interface())
				if w.Error() != nil {
					return
				}
			}
		case reflect.Struct:
			rt := rv.Type()
			var fields []struct {
				name  string
				value interface{}
			}
			for i := 0; i < rv.NumField(); i++ {
				fieldVal := rv.Field(i)
				sf := rt.Field(i)
				if sf.PkgPath != "" {
					continue // unexported
				}
				bsatnTag := sf.Tag.Get("bsatn")
				if bsatnTag == "-" {
					continue // explicitly ignored
				}
				name := sf.Name
				if bsatnTag != "" {
					commaIdx := strings.Index(bsatnTag, ",")
					if commaIdx >= 0 {
						name = bsatnTag[:commaIdx]
					} else {
						name = bsatnTag
					}
					if name == "" {
						name = sf.Name // empty before comma means use default
					}
				}
				fields = append(fields, struct {
					name  string
					value interface{}
				}{name, fieldVal.Interface()})
			}
			sort.Slice(fields, func(i, j int) bool { return fields[i].name < fields[j].name })
			w.WriteStructHeader(len(fields))
			for _, f := range fields {
				if w.Error() != nil {
					return
				}
				w.WriteFieldName(f.name)
				if w.Error() != nil {
					return
				}
				marshalRecursive(w, f.value)
				if w.Error() != nil {
					return
				}
			}
		default:
			w.recordError(fmt.Errorf("bsatn: unsupported type for marshalling: %T (Kind: %s)", v, rv.Kind()))
		}
	}
}

// Note: MarshalAlgebraicType also needs to be refactored to accept a Writer
// or its current implementation needs to be compatible with this new Marshal structure.
// For now, the AlgebraicType case in marshalRecursive has a temporary workaround.
