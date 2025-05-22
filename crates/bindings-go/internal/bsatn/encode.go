package bsatn

import (
	"encoding/binary"
	"math"
	"reflect"
	"sort"
	"unicode/utf8"
)

// Marshal encodes v into BSATN byte slice (supports basic primitives and string/[]byte)
func Marshal(v interface{}) ([]byte, error) {
	switch val := v.(type) {
	case bool:
		if val {
			return []byte{TagBoolTrue}, nil
		}
		return []byte{TagBoolFalse}, nil
	case uint8:
		return []byte{TagU8, val}, nil
	case int8:
		return []byte{TagI8, byte(val)}, nil
	case uint16:
		buf := make([]byte, 1+2)
		buf[0] = TagU16
		binary.LittleEndian.PutUint16(buf[1:], val)
		return buf, nil
	case int16:
		buf := make([]byte, 1+2)
		buf[0] = TagI16
		binary.LittleEndian.PutUint16(buf[1:], uint16(val))
		return buf, nil
	case uint32:
		buf := make([]byte, 1+4)
		buf[0] = TagU32
		binary.LittleEndian.PutUint32(buf[1:], val)
		return buf, nil
	case int32:
		buf := make([]byte, 1+4)
		buf[0] = TagI32
		binary.LittleEndian.PutUint32(buf[1:], uint32(val))
		return buf, nil
	case uint64:
		buf := make([]byte, 1+8)
		buf[0] = TagU64
		binary.LittleEndian.PutUint64(buf[1:], val)
		return buf, nil
	case int64:
		buf := make([]byte, 1+8)
		buf[0] = TagI64
		binary.LittleEndian.PutUint64(buf[1:], uint64(val))
		return buf, nil
	case float32:
		buf := make([]byte, 1+4)
		buf[0] = TagF32
		binary.LittleEndian.PutUint32(buf[1:], math.Float32bits(val))
		return buf, nil
	case float64:
		buf := make([]byte, 1+8)
		buf[0] = TagF64
		binary.LittleEndian.PutUint64(buf[1:], math.Float64bits(val))
		return buf, nil
	case string:
		if !utf8.ValidString(val) {
			return nil, ErrInvalidUTF8
		}
		strBytes := []byte(val)
		buf := make([]byte, 1+4+len(strBytes))
		buf[0] = TagString
		binary.LittleEndian.PutUint32(buf[1:], uint32(len(strBytes)))
		copy(buf[5:], strBytes)
		return buf, nil
	case []byte:
		buf := make([]byte, 1+4+len(val))
		buf[0] = TagBytes
		binary.LittleEndian.PutUint32(buf[1:], uint32(len(val)))
		copy(buf[5:], val)
		return buf, nil
	case []interface{}:
		// encode list: tag, length, then each element
		var elements [][]byte
		var total int
		for _, e := range val {
			enc, err := Marshal(e)
			if err != nil {
				return nil, err
			}
			elements = append(elements, enc)
			total += len(enc)
		}
		buf := make([]byte, 1+4+total)
		buf[0] = TagList
		binary.LittleEndian.PutUint32(buf[1:], uint32(len(val)))
		off := 5
		for _, enc := range elements {
			copy(buf[off:], enc)
			off += len(enc)
		}
		return buf, nil
	case nil:
		return []byte{TagOptionNone}, nil
	case Variant:
		payload := []byte{}
		if val.Value != nil {
			enc, err := Marshal(val.Value)
			if err != nil {
				return nil, err
			}
			payload = enc
		}
		buf := make([]byte, 1+4+len(payload))
		buf[0] = TagEnum
		binary.LittleEndian.PutUint32(buf[1:], val.Index)
		copy(buf[5:], payload)
		return buf, nil
	default:
		// fallback: if pointer, deref
		rv := reflect.ValueOf(v)
		if rv.Kind() == reflect.Ptr {
			if rv.IsNil() {
				return []byte{TagOptionNone}, nil
			}
			somePayload, err := Marshal(rv.Elem().Interface())
			if err != nil {
				return nil, err
			}
			buf := make([]byte, 1+len(somePayload))
			buf[0] = TagOptionSome
			copy(buf[1:], somePayload)
			return buf, nil
		}
		if rv.Kind() == reflect.Slice {
			sliceLen := rv.Len()
			// treat []interface{} earlier; here ensure element kind not Interface
			if rv.Type().Elem().Kind() != reflect.Interface {
				// encode as TagArray
				total := 0
				encodedElems := make([][]byte, sliceLen)
				for i := 0; i < sliceLen; i++ {
					enc, err := Marshal(rv.Index(i).Interface())
					if err != nil {
						return nil, err
					}
					encodedElems[i] = enc
					total += len(enc)
				}
				buf := make([]byte, 1+4+total)
				buf[0] = TagArray
				binary.LittleEndian.PutUint32(buf[1:], uint32(sliceLen))
				off := 5
				for _, enc := range encodedElems {
					copy(buf[off:], enc)
					off += len(enc)
				}
				return buf, nil
			}
			// otherwise fallback to TagList logic via []interface{}
			sliceIfc := make([]interface{}, sliceLen)
			for i := 0; i < sliceLen; i++ {
				sliceIfc[i] = rv.Index(i).Interface()
			}
			return Marshal(sliceIfc)
		}
		if rv.Kind() == reflect.Array {
			length := rv.Len()
			encodedElems := make([][]byte, length)
			total := 0
			for i := 0; i < length; i++ {
				enc, err := Marshal(rv.Index(i).Interface())
				if err != nil {
					return nil, err
				}
				encodedElems[i] = enc
				total += len(enc)
			}
			buf := make([]byte, 1+4+total)
			buf[0] = TagArray
			binary.LittleEndian.PutUint32(buf[1:], uint32(length))
			off := 5
			for _, enc := range encodedElems {
				copy(buf[off:], enc)
				off += len(enc)
			}
			return buf, nil
		}
		// handle struct via reflection
		rt := rv.Type()
		if rv.Kind() == reflect.Struct {
			fieldCount := rv.NumField()
			// first pass encode each field
			type fieldEntry struct {
				name string
				enc  []byte
			}
			var entries []fieldEntry
			var total int
			for i := 0; i < fieldCount; i++ {
				field := rv.Field(i)
				sf := rt.Field(i)
				// skip unexported
				if sf.PkgPath != "" {
					continue
				}

				tagName := sf.Tag.Get("bsatn")
				if tagName == "-" {
					continue
				}
				fieldName := sf.Name
				if tagName != "" {
					fieldName = tagName
				}
				// encode name len + name
				nameBytes := []byte(fieldName)
				if len(nameBytes) > 255 {
					return nil, ErrInvalidTag
				}
				valEnc, err := Marshal(field.Interface())
				if err != nil {
					return nil, err
				}
				entry := make([]byte, 1+len(nameBytes)+len(valEnc))
				entry[0] = byte(len(nameBytes))
				copy(entry[1:], nameBytes)
				copy(entry[1+len(nameBytes):], valEnc)
				entries = append(entries, fieldEntry{name: fieldName, enc: entry})
				total += len(entry)
			}
			// sort entries by name for deterministic order
			sort.Slice(entries, func(i, j int) bool { return entries[i].name < entries[j].name })
			buf := make([]byte, 1+4+total)
			buf[0] = TagStruct
			binary.LittleEndian.PutUint32(buf[1:], uint32(len(entries)))
			off := 5
			for _, e := range entries {
				copy(buf[off:], e.enc)
				off += len(e.enc)
			}
			return buf, nil
		}
	}
	return nil, ErrInvalidTag
}
