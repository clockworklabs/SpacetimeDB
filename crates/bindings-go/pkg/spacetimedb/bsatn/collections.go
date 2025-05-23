package bsatn

import (
	"fmt"
	"io"
	"reflect"
)

// Collection Types and Complex Data Structures

// Array Encoding/Decoding

// EncodeArray encodes an array of values using the provided element encoder
func EncodeArray[T any](w io.Writer, array []T, encodeElement func(io.Writer, T) error) error {
	// Encode array length
	if err := EncodeU32(w, uint32(len(array))); err != nil {
		return &EncodingError{Type: "array", Reason: "failed to encode length", Err: err}
	}

	// Encode each element
	for i, element := range array {
		if err := encodeElement(w, element); err != nil {
			return &EncodingError{Type: "array", Reason: fmt.Sprintf("failed to encode element %d", i), Err: err}
		}
	}

	return nil
}

// DecodeArray decodes an array of values using the provided element decoder
func DecodeArray[T any](r io.Reader, decodeElement func(io.Reader) (T, error)) ([]T, error) {
	// Decode array length
	length, err := DecodeU32(r)
	if err != nil {
		return nil, &DecodingError{Type: "array", Reason: "failed to decode length", Err: err}
	}

	// Sanity check for length
	if length > 1<<20 { // 1M elements limit
		return nil, &DecodingError{Type: "array", Reason: fmt.Sprintf("array too long: %d elements", length)}
	}

	// Decode each element
	result := make([]T, length)
	for i := uint32(0); i < length; i++ {
		element, err := decodeElement(r)
		if err != nil {
			return nil, &DecodingError{Type: "array", Reason: fmt.Sprintf("failed to decode element %d", i), Err: err}
		}
		result[i] = element
	}

	return result, nil
}

// SizeArray calculates the size of an array when serialized
func SizeArray[T any](array []T, sizeElement func(T) int) int {
	size := SizeU32() // Length prefix
	for _, element := range array {
		size += sizeElement(element)
	}
	return size
}

// Optional (Nullable) Types

// EncodeOptional encodes an optional value (Go pointer)
func EncodeOptional[T any](w io.Writer, opt *T, encodeValue func(io.Writer, T) error) error {
	if opt == nil {
		// Tag = 0 for None
		return EncodeU8(w, 0)
	} else {
		// Tag = 1 for Some
		if err := EncodeU8(w, 1); err != nil {
			return &EncodingError{Type: "optional", Reason: "failed to encode Some tag", Err: err}
		}
		return encodeValue(w, *opt)
	}
}

// DecodeOptional decodes an optional value (Go pointer)
func DecodeOptional[T any](r io.Reader, decodeValue func(io.Reader) (T, error)) (*T, error) {
	tag, err := DecodeU8(r)
	if err != nil {
		return nil, &DecodingError{Type: "optional", Reason: "failed to decode tag", Err: err}
	}

	switch tag {
	case 0: // None
		return nil, nil
	case 1: // Some
		value, err := decodeValue(r)
		if err != nil {
			return nil, &DecodingError{Type: "optional", Reason: "failed to decode value", Err: err}
		}
		return &value, nil
	default:
		return nil, &DecodingError{Type: "optional", Reason: fmt.Sprintf("invalid tag: %d", tag)}
	}
}

// SizeOptional calculates the size of an optional when serialized
func SizeOptional[T any](opt *T, sizeValue func(T) int) int {
	if opt == nil {
		return 1 // Just the tag
	} else {
		return 1 + sizeValue(*opt) // Tag + value
	}
}

// Map Encoding/Decoding

// EncodeMap encodes a map as an array of key-value pairs
func EncodeMap[K comparable, V any](w io.Writer, m map[K]V, encodeKey func(io.Writer, K) error, encodeValue func(io.Writer, V) error) error {
	// Encode map length
	if err := EncodeU32(w, uint32(len(m))); err != nil {
		return &EncodingError{Type: "map", Reason: "failed to encode length", Err: err}
	}

	// Encode each key-value pair
	i := 0
	for key, value := range m {
		// Encode key
		if err := encodeKey(w, key); err != nil {
			return &EncodingError{Type: "map", Reason: fmt.Sprintf("failed to encode key %d", i), Err: err}
		}
		// Encode value
		if err := encodeValue(w, value); err != nil {
			return &EncodingError{Type: "map", Reason: fmt.Sprintf("failed to encode value %d", i), Err: err}
		}
		i++
	}

	return nil
}

// DecodeMap decodes a map from an array of key-value pairs
func DecodeMap[K comparable, V any](r io.Reader, decodeKey func(io.Reader) (K, error), decodeValue func(io.Reader) (V, error)) (map[K]V, error) {
	// Decode map length
	length, err := DecodeU32(r)
	if err != nil {
		return nil, &DecodingError{Type: "map", Reason: "failed to decode length", Err: err}
	}

	// Sanity check for length
	if length > 1<<20 { // 1M entries limit
		return nil, &DecodingError{Type: "map", Reason: fmt.Sprintf("map too long: %d entries", length)}
	}

	// Decode each key-value pair
	result := make(map[K]V, length)
	for i := uint32(0); i < length; i++ {
		// Decode key
		key, err := decodeKey(r)
		if err != nil {
			return nil, &DecodingError{Type: "map", Reason: fmt.Sprintf("failed to decode key %d", i), Err: err}
		}
		// Decode value
		value, err := decodeValue(r)
		if err != nil {
			return nil, &DecodingError{Type: "map", Reason: fmt.Sprintf("failed to decode value %d", i), Err: err}
		}
		result[key] = value
	}

	return result, nil
}

// SizeMap calculates the size of a map when serialized
func SizeMap[K comparable, V any](m map[K]V, sizeKey func(K) int, sizeValue func(V) int) int {
	size := SizeU32() // Length prefix
	for key, value := range m {
		size += sizeKey(key)
		size += sizeValue(value)
	}
	return size
}

// Struct-like Encoding using Reflection (for prototyping)

// EncodeStruct encodes a struct using reflection (slower but convenient)
func EncodeStruct(w io.Writer, v interface{}) error {
	rv := reflect.ValueOf(v)
	if rv.Kind() == reflect.Ptr {
		rv = rv.Elem()
	}

	if rv.Kind() != reflect.Struct {
		return &EncodingError{Type: "struct", Reason: "value is not a struct"}
	}

	rt := rv.Type()
	numFields := rv.NumField()

	// Encode number of fields
	if err := EncodeU32(w, uint32(numFields)); err != nil {
		return &EncodingError{Type: "struct", Reason: "failed to encode field count", Err: err}
	}

	// Encode each field
	for i := 0; i < numFields; i++ {
		field := rv.Field(i)
		fieldType := rt.Field(i)

		// Skip unexported fields
		if !field.CanInterface() {
			continue
		}

		// Encode field name
		if err := EncodeString(w, fieldType.Name); err != nil {
			return &EncodingError{Type: "struct", Reason: fmt.Sprintf("failed to encode field name %s", fieldType.Name), Err: err}
		}

		// Encode field value based on type
		if err := encodeValue(w, field.Interface()); err != nil {
			return &EncodingError{Type: "struct", Reason: fmt.Sprintf("failed to encode field %s", fieldType.Name), Err: err}
		}
	}

	return nil
}

// encodeValue is a helper that encodes a value based on its type
func encodeValue(w io.Writer, v interface{}) error {
	switch val := v.(type) {
	case uint8:
		return EncodeU8(w, val)
	case uint16:
		return EncodeU16(w, val)
	case uint32:
		return EncodeU32(w, val)
	case uint64:
		return EncodeU64(w, val)
	case int8:
		return EncodeI8(w, val)
	case int16:
		return EncodeI16(w, val)
	case int32:
		return EncodeI32(w, val)
	case int64:
		return EncodeI64(w, val)
	case float32:
		return EncodeF32(w, val)
	case float64:
		return EncodeF64(w, val)
	case bool:
		return EncodeBool(w, val)
	case string:
		return EncodeString(w, val)
	case []byte:
		return EncodeBytes(w, val)
	default:
		return &EncodingError{Type: "unknown", Reason: fmt.Sprintf("unsupported type: %T", v)}
	}
}

// Convenience functions for common array types

// EncodeU32Array encodes an array of uint32 values
func EncodeU32Array(w io.Writer, array []uint32) error {
	return EncodeArray(w, array, EncodeU32)
}

// DecodeU32Array decodes an array of uint32 values
func DecodeU32Array(r io.Reader) ([]uint32, error) {
	return DecodeArray(r, DecodeU32)
}

// EncodeStringArray encodes an array of string values
func EncodeStringArray(w io.Writer, array []string) error {
	return EncodeArray(w, array, EncodeString)
}

// DecodeStringArray decodes an array of string values
func DecodeStringArray(r io.Reader) ([]string, error) {
	return DecodeArray(r, DecodeString)
}

// EncodeF64Array encodes an array of float64 values
func EncodeF64Array(w io.Writer, array []float64) error {
	return EncodeArray(w, array, EncodeF64)
}

// DecodeF64Array decodes an array of float64 values
func DecodeF64Array(r io.Reader) ([]float64, error) {
	return DecodeArray(r, DecodeF64)
}

// Size calculation for common array types

// SizeU32Array calculates the size of a uint32 array when serialized
func SizeU32Array(array []uint32) int {
	return SizeArray(array, func(uint32) int { return SizeU32() })
}

// SizeStringArray calculates the size of a string array when serialized
func SizeStringArray(array []string) int {
	return SizeArray(array, SizeString)
}

// SizeF64Array calculates the size of a float64 array when serialized
func SizeF64Array(array []float64) int {
	return SizeArray(array, func(float64) int { return SizeF64() })
}

// Typed Array Codecs

// U32ArrayCodec provides BSATN encoding/decoding for uint32 arrays
type U32ArrayCodec struct {
	Value []uint32
}

func (ac *U32ArrayCodec) Encode(w io.Writer) error {
	return EncodeU32Array(w, ac.Value)
}

func (ac *U32ArrayCodec) Decode(r io.Reader) error {
	val, err := DecodeU32Array(r)
	if err != nil {
		return err
	}
	ac.Value = val
	return nil
}

func (ac *U32ArrayCodec) BsatnSize() int {
	return SizeU32Array(ac.Value)
}

// StringArrayCodec provides BSATN encoding/decoding for string arrays
type StringArrayCodec struct {
	Value []string
}

func (sac *StringArrayCodec) Encode(w io.Writer) error {
	return EncodeStringArray(w, sac.Value)
}

func (sac *StringArrayCodec) Decode(r io.Reader) error {
	val, err := DecodeStringArray(r)
	if err != nil {
		return err
	}
	sac.Value = val
	return nil
}

func (sac *StringArrayCodec) BsatnSize() int {
	return SizeStringArray(sac.Value)
}

// Utility functions for arrays

// U32ArrayToBytes serializes a uint32 array to bytes
func U32ArrayToBytes(array []uint32) ([]byte, error) {
	return ToBytes(func(w io.Writer) error {
		return EncodeU32Array(w, array)
	})
}

// U32ArrayFromBytes deserializes a uint32 array from bytes
func U32ArrayFromBytes(data []byte) ([]uint32, error) {
	var array []uint32
	err := FromBytes(data, func(r io.Reader) error {
		var err error
		array, err = DecodeU32Array(r)
		return err
	})
	return array, err
}

// StringArrayToBytes serializes a string array to bytes
func StringArrayToBytes(array []string) ([]byte, error) {
	return ToBytes(func(w io.Writer) error {
		return EncodeStringArray(w, array)
	})
}

// StringArrayFromBytes deserializes a string array from bytes
func StringArrayFromBytes(data []byte) ([]string, error) {
	var array []string
	err := FromBytes(data, func(r io.Reader) error {
		var err error
		array, err = DecodeStringArray(r)
		return err
	})
	return array, err
}
