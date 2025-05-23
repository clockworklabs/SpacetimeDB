package bsatn

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"io"
	"math"
)

// BSATN (Binary SpaceTime Arithmetic Type Notation) Serialization Framework
// This provides efficient binary serialization for SpacetimeDB types

// Encoder defines the interface for BSATN encoding
type Encoder interface {
	// Encode writes the value to the writer in BSATN format
	Encode(w io.Writer) error
}

// Decoder defines the interface for BSATN decoding
type Decoder interface {
	// Decode reads and populates the value from the reader in BSATN format
	Decode(r io.Reader) error
}

// Codec defines a type that can both encode and decode
type Codec interface {
	Encoder
	Decoder
}

// Sizer defines the interface for calculating serialized size
type Sizer interface {
	// BsatnSize returns the size in bytes when serialized
	BsatnSize() int
}

// TypedCodec combines Codec with size calculation
type TypedCodec interface {
	Codec
	Sizer
}

// EncodingError represents an error during BSATN encoding
type EncodingError struct {
	Type   string
	Reason string
	Err    error
}

func (e *EncodingError) Error() string {
	if e.Err != nil {
		return fmt.Sprintf("bsatn encoding error for %s: %s: %v", e.Type, e.Reason, e.Err)
	}
	return fmt.Sprintf("bsatn encoding error for %s: %s", e.Type, e.Reason)
}

func (e *EncodingError) Unwrap() error {
	return e.Err
}

// DecodingError represents an error during BSATN decoding
type DecodingError struct {
	Type   string
	Reason string
	Err    error
}

func (d *DecodingError) Error() string {
	if d.Err != nil {
		return fmt.Sprintf("bsatn decoding error for %s: %s: %v", d.Type, d.Reason, d.Err)
	}
	return fmt.Sprintf("bsatn decoding error for %s: %s", d.Type, d.Reason)
}

func (d *DecodingError) Unwrap() error {
	return d.Err
}

// Primitive Type Encoders

// EncodeU8 encodes a uint8 value in BSATN format
func EncodeU8(w io.Writer, val uint8) error {
	_, err := w.Write([]byte{val})
	if err != nil {
		return &EncodingError{Type: "u8", Reason: "write failed", Err: err}
	}
	return nil
}

// DecodeU8 decodes a uint8 value from BSATN format
func DecodeU8(r io.Reader) (uint8, error) {
	buf := make([]byte, 1)
	_, err := io.ReadFull(r, buf)
	if err != nil {
		return 0, &DecodingError{Type: "u8", Reason: "read failed", Err: err}
	}
	return buf[0], nil
}

// EncodeU16 encodes a uint16 value in BSATN format (little-endian)
func EncodeU16(w io.Writer, val uint16) error {
	buf := make([]byte, 2)
	binary.LittleEndian.PutUint16(buf, val)
	_, err := w.Write(buf)
	if err != nil {
		return &EncodingError{Type: "u16", Reason: "write failed", Err: err}
	}
	return nil
}

// DecodeU16 decodes a uint16 value from BSATN format (little-endian)
func DecodeU16(r io.Reader) (uint16, error) {
	buf := make([]byte, 2)
	_, err := io.ReadFull(r, buf)
	if err != nil {
		return 0, &DecodingError{Type: "u16", Reason: "read failed", Err: err}
	}
	return binary.LittleEndian.Uint16(buf), nil
}

// EncodeU32 encodes a uint32 value in BSATN format (little-endian)
func EncodeU32(w io.Writer, val uint32) error {
	buf := make([]byte, 4)
	binary.LittleEndian.PutUint32(buf, val)
	_, err := w.Write(buf)
	if err != nil {
		return &EncodingError{Type: "u32", Reason: "write failed", Err: err}
	}
	return nil
}

// DecodeU32 decodes a uint32 value from BSATN format (little-endian)
func DecodeU32(r io.Reader) (uint32, error) {
	buf := make([]byte, 4)
	_, err := io.ReadFull(r, buf)
	if err != nil {
		return 0, &DecodingError{Type: "u32", Reason: "read failed", Err: err}
	}
	return binary.LittleEndian.Uint32(buf), nil
}

// EncodeU64 encodes a uint64 value in BSATN format (little-endian)
func EncodeU64(w io.Writer, val uint64) error {
	buf := make([]byte, 8)
	binary.LittleEndian.PutUint64(buf, val)
	_, err := w.Write(buf)
	if err != nil {
		return &EncodingError{Type: "u64", Reason: "write failed", Err: err}
	}
	return nil
}

// DecodeU64 decodes a uint64 value from BSATN format (little-endian)
func DecodeU64(r io.Reader) (uint64, error) {
	buf := make([]byte, 8)
	_, err := io.ReadFull(r, buf)
	if err != nil {
		return 0, &DecodingError{Type: "u64", Reason: "read failed", Err: err}
	}
	return binary.LittleEndian.Uint64(buf), nil
}

// EncodeI8 encodes an int8 value in BSATN format
func EncodeI8(w io.Writer, val int8) error {
	return EncodeU8(w, uint8(val))
}

// DecodeI8 decodes an int8 value from BSATN format
func DecodeI8(r io.Reader) (int8, error) {
	val, err := DecodeU8(r)
	if err != nil {
		return 0, err
	}
	return int8(val), nil
}

// EncodeI16 encodes an int16 value in BSATN format (little-endian)
func EncodeI16(w io.Writer, val int16) error {
	return EncodeU16(w, uint16(val))
}

// DecodeI16 decodes an int16 value from BSATN format (little-endian)
func DecodeI16(r io.Reader) (int16, error) {
	val, err := DecodeU16(r)
	if err != nil {
		return 0, err
	}
	return int16(val), nil
}

// EncodeI32 encodes an int32 value in BSATN format (little-endian)
func EncodeI32(w io.Writer, val int32) error {
	return EncodeU32(w, uint32(val))
}

// DecodeI32 decodes an int32 value from BSATN format (little-endian)
func DecodeI32(r io.Reader) (int32, error) {
	val, err := DecodeU32(r)
	if err != nil {
		return 0, err
	}
	return int32(val), nil
}

// EncodeI64 encodes an int64 value in BSATN format (little-endian)
func EncodeI64(w io.Writer, val int64) error {
	return EncodeU64(w, uint64(val))
}

// DecodeI64 decodes an int64 value from BSATN format (little-endian)
func DecodeI64(r io.Reader) (int64, error) {
	val, err := DecodeU64(r)
	if err != nil {
		return 0, err
	}
	return int64(val), nil
}

// EncodeF32 encodes a float32 value in BSATN format (IEEE 754, little-endian)
func EncodeF32(w io.Writer, val float32) error {
	bits := math.Float32bits(val)
	return EncodeU32(w, bits)
}

// DecodeF32 decodes a float32 value from BSATN format (IEEE 754, little-endian)
func DecodeF32(r io.Reader) (float32, error) {
	bits, err := DecodeU32(r)
	if err != nil {
		return 0, err
	}
	return math.Float32frombits(bits), nil
}

// EncodeF64 encodes a float64 value in BSATN format (IEEE 754, little-endian)
func EncodeF64(w io.Writer, val float64) error {
	bits := math.Float64bits(val)
	return EncodeU64(w, bits)
}

// DecodeF64 decodes a float64 value from BSATN format (IEEE 754, little-endian)
func DecodeF64(r io.Reader) (float64, error) {
	bits, err := DecodeU64(r)
	if err != nil {
		return 0, err
	}
	return math.Float64frombits(bits), nil
}

// EncodeBool encodes a bool value in BSATN format (0 = false, 1 = true)
func EncodeBool(w io.Writer, val bool) error {
	var b uint8
	if val {
		b = 1
	}
	return EncodeU8(w, b)
}

// DecodeBool decodes a bool value from BSATN format (0 = false, 1 = true)
func DecodeBool(r io.Reader) (bool, error) {
	val, err := DecodeU8(r)
	if err != nil {
		return false, err
	}
	if val > 1 {
		return false, &DecodingError{Type: "bool", Reason: fmt.Sprintf("invalid bool value: %d", val)}
	}
	return val == 1, nil
}

// EncodeString encodes a string value in BSATN format (length-prefixed UTF-8)
func EncodeString(w io.Writer, val string) error {
	data := []byte(val)
	// Encode length as u32
	if err := EncodeU32(w, uint32(len(data))); err != nil {
		return &EncodingError{Type: "string", Reason: "failed to encode length", Err: err}
	}
	// Encode string data
	_, err := w.Write(data)
	if err != nil {
		return &EncodingError{Type: "string", Reason: "failed to write data", Err: err}
	}
	return nil
}

// DecodeString decodes a string value from BSATN format (length-prefixed UTF-8)
func DecodeString(r io.Reader) (string, error) {
	// Decode length
	length, err := DecodeU32(r)
	if err != nil {
		return "", &DecodingError{Type: "string", Reason: "failed to decode length", Err: err}
	}

	// Sanity check for length
	if length > 1<<24 { // 16MB limit
		return "", &DecodingError{Type: "string", Reason: fmt.Sprintf("string too long: %d bytes", length)}
	}

	// Decode string data
	data := make([]byte, length)
	_, err = io.ReadFull(r, data)
	if err != nil {
		return "", &DecodingError{Type: "string", Reason: "failed to read data", Err: err}
	}

	return string(data), nil
}

// EncodeBytes encodes a byte slice in BSATN format (length-prefixed)
func EncodeBytes(w io.Writer, val []byte) error {
	// Encode length
	if err := EncodeU32(w, uint32(len(val))); err != nil {
		return &EncodingError{Type: "bytes", Reason: "failed to encode length", Err: err}
	}
	// Encode byte data
	_, err := w.Write(val)
	if err != nil {
		return &EncodingError{Type: "bytes", Reason: "failed to write data", Err: err}
	}
	return nil
}

// DecodeBytes decodes a byte slice from BSATN format (length-prefixed)
func DecodeBytes(r io.Reader) ([]byte, error) {
	// Decode length
	length, err := DecodeU32(r)
	if err != nil {
		return nil, &DecodingError{Type: "bytes", Reason: "failed to decode length", Err: err}
	}

	// Sanity check for length
	if length > 1<<26 { // 64MB limit
		return nil, &DecodingError{Type: "bytes", Reason: fmt.Sprintf("byte array too long: %d bytes", length)}
	}

	// Decode byte data
	data := make([]byte, length)
	_, err = io.ReadFull(r, data)
	if err != nil {
		return nil, &DecodingError{Type: "bytes", Reason: "failed to read data", Err: err}
	}

	return data, nil
}

// Size calculation utilities

// SizeU8 returns the size of a u8 when serialized (always 1)
func SizeU8() int { return 1 }

// SizeU16 returns the size of a u16 when serialized (always 2)
func SizeU16() int { return 2 }

// SizeU32 returns the size of a u32 when serialized (always 4)
func SizeU32() int { return 4 }

// SizeU64 returns the size of a u64 when serialized (always 8)
func SizeU64() int { return 8 }

// SizeI8 returns the size of an i8 when serialized (always 1)
func SizeI8() int { return 1 }

// SizeI16 returns the size of an i16 when serialized (always 2)
func SizeI16() int { return 2 }

// SizeI32 returns the size of an i32 when serialized (always 4)
func SizeI32() int { return 4 }

// SizeI64 returns the size of an i64 when serialized (always 8)
func SizeI64() int { return 8 }

// SizeF32 returns the size of an f32 when serialized (always 4)
func SizeF32() int { return 4 }

// SizeF64 returns the size of an f64 when serialized (always 8)
func SizeF64() int { return 8 }

// SizeBool returns the size of a bool when serialized (always 1)
func SizeBool() int { return 1 }

// SizeString returns the size of a string when serialized (length + data)
func SizeString(val string) int {
	return SizeU32() + len([]byte(val))
}

// SizeBytes returns the size of a byte slice when serialized (length + data)
func SizeBytes(val []byte) int {
	return SizeU32() + len(val)
}

// Utility functions for common operations

// ToBytes serializes a value to bytes using the provided encoder
func ToBytes(encoder func(io.Writer) error) ([]byte, error) {
	buf := &bytes.Buffer{}
	if err := encoder(buf); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

// FromBytes deserializes a value from bytes using the provided decoder
func FromBytes(data []byte, decoder func(io.Reader) error) error {
	buf := bytes.NewReader(data)
	return decoder(buf)
}

// Constants for type tags (used in variant encoding)
const (
	TypeTagU8     = 0
	TypeTagU16    = 1
	TypeTagU32    = 2
	TypeTagU64    = 3
	TypeTagU128   = 4
	TypeTagU256   = 5
	TypeTagI8     = 6
	TypeTagI16    = 7
	TypeTagI32    = 8
	TypeTagI64    = 9
	TypeTagI128   = 10
	TypeTagI256   = 11
	TypeTagF32    = 12
	TypeTagF64    = 13
	TypeTagBool   = 14
	TypeTagString = 15
	TypeTagBytes  = 16
)
