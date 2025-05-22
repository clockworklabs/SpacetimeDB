package bsatn

import (
	"encoding/binary"
	"math"
)

// Unmarshal decodes BSATN-encoded data into Go primitives (bool, numbers, string, []byte).
func Unmarshal(buf []byte) (interface{}, error) {
	if len(buf) == 0 {
		return nil, ErrBufferTooSmall
	}
	tag := buf[0]
	switch tag {
	case TagBoolFalse:
		return false, nil
	case TagBoolTrue:
		return true, nil
	case TagU8:
		if len(buf) < 2 {
			return nil, ErrBufferTooSmall
		}
		return buf[1], nil
	case TagI8:
		if len(buf) < 2 {
			return nil, ErrBufferTooSmall
		}
		return int8(buf[1]), nil
	case TagU16:
		if len(buf) < 3 {
			return nil, ErrBufferTooSmall
		}
		return binary.LittleEndian.Uint16(buf[1:3]), nil
	case TagI16:
		if len(buf) < 3 {
			return nil, ErrBufferTooSmall
		}
		return int16(binary.LittleEndian.Uint16(buf[1:3])), nil
	case TagU32:
		if len(buf) < 5 {
			return nil, ErrBufferTooSmall
		}
		return binary.LittleEndian.Uint32(buf[1:5]), nil
	case TagI32:
		if len(buf) < 5 {
			return nil, ErrBufferTooSmall
		}
		return int32(binary.LittleEndian.Uint32(buf[1:5])), nil
	case TagU64:
		if len(buf) < 9 {
			return nil, ErrBufferTooSmall
		}
		return binary.LittleEndian.Uint64(buf[1:9]), nil
	case TagI64:
		if len(buf) < 9 {
			return nil, ErrBufferTooSmall
		}
		return int64(binary.LittleEndian.Uint64(buf[1:9])), nil
	case TagF32:
		if len(buf) < 5 {
			return nil, ErrBufferTooSmall
		}
		bits := binary.LittleEndian.Uint32(buf[1:5])
		return math.Float32frombits(bits), nil
	case TagF64:
		if len(buf) < 9 {
			return nil, ErrBufferTooSmall
		}
		bits := binary.LittleEndian.Uint64(buf[1:9])
		return math.Float64frombits(bits), nil
	case TagString, TagBytes:
		if len(buf) < 5 {
			return nil, ErrBufferTooSmall
		}
		size := binary.LittleEndian.Uint32(buf[1:5])
		if uint32(len(buf[5:])) < size {
			return nil, ErrBufferTooSmall
		}
		data := buf[5 : 5+size]
		if tag == TagString {
			return string(data), nil
		}
		return append([]byte(nil), data...), nil
	case TagList:
		if len(buf) < 5 {
			return nil, ErrBufferTooSmall
		}
		count := binary.LittleEndian.Uint32(buf[1:5])
		items := make([]interface{}, 0, count)
		offset := 5
		for i := uint32(0); i < count; i++ {
			if offset >= len(buf) {
				return nil, ErrBufferTooSmall
			}
			elem, err := Unmarshal(buf[offset:])
			if err != nil {
				return nil, err
			}
			// re-marshal to know size of element
			enc, _ := Marshal(elem)
			offset += len(enc)
			items = append(items, elem)
		}
		return items, nil
	case TagArray:
		if len(buf) < 5 {
			return nil, ErrBufferTooSmall
		}
		count := binary.LittleEndian.Uint32(buf[1:5])
		offset := 5
		items := make([]interface{}, 0, count)
		for i := uint32(0); i < count; i++ {
			if offset >= len(buf) {
				return nil, ErrBufferTooSmall
			}
			elem, err := Unmarshal(buf[offset:])
			if err != nil {
				return nil, err
			}
			enc, _ := Marshal(elem)
			offset += len(enc)
			items = append(items, elem)
		}
		return items, nil
	case TagOptionNone:
		return nil, nil
	case TagOptionSome:
		if len(buf) < 2 {
			return nil, ErrBufferTooSmall
		}
		val, err := Unmarshal(buf[1:])
		if err != nil {
			return nil, err
		}
		return &val, nil
	case TagStruct:
		if len(buf) < 5 {
			return nil, ErrBufferTooSmall
		}
		fieldCount := binary.LittleEndian.Uint32(buf[1:5])
		offset := 5
		m := make(map[string]interface{}, fieldCount)
		for i := uint32(0); i < fieldCount; i++ {
			if offset >= len(buf) {
				return nil, ErrBufferTooSmall
			}
			nameLen := int(buf[offset])
			offset++
			if offset+nameLen > len(buf) {
				return nil, ErrBufferTooSmall
			}
			name := string(buf[offset : offset+nameLen])
			offset += nameLen
			val, err := Unmarshal(buf[offset:])
			if err != nil {
				return nil, err
			}
			enc, _ := Marshal(val)
			offset += len(enc)
			m[name] = val
		}
		return m, nil
	case TagEnum:
		if len(buf) < 5 {
			return nil, ErrBufferTooSmall
		}
		idx := binary.LittleEndian.Uint32(buf[1:5])
		if len(buf) == 5 { // unit variant
			return Variant{Index: idx}, nil
		}
		val, err := Unmarshal(buf[5:])
		if err != nil {
			return nil, err
		}
		return Variant{Index: idx, Value: val}, nil
	default:
		return nil, ErrInvalidTag
	}
}
