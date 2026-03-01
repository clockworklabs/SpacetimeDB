package bsatn

// EncodeBool encodes a bool to BSATN bytes.
func EncodeBool(v bool) []byte {
	w := NewWriter(1)
	w.PutBool(v)
	return w.Bytes()
}

// DecodeBool decodes a bool from BSATN bytes.
func DecodeBool(data []byte) (bool, error) {
	return NewReader(data).GetBool()
}

// EncodeU8 encodes a uint8 to BSATN bytes.
func EncodeU8(v uint8) []byte {
	w := NewWriter(1)
	w.PutU8(v)
	return w.Bytes()
}

// DecodeU8 decodes a uint8 from BSATN bytes.
func DecodeU8(data []byte) (uint8, error) {
	return NewReader(data).GetU8()
}

// EncodeU16 encodes a uint16 to BSATN bytes.
func EncodeU16(v uint16) []byte {
	w := NewWriter(2)
	w.PutU16(v)
	return w.Bytes()
}

// DecodeU16 decodes a uint16 from BSATN bytes.
func DecodeU16(data []byte) (uint16, error) {
	return NewReader(data).GetU16()
}

// EncodeU32 encodes a uint32 to BSATN bytes.
func EncodeU32(v uint32) []byte {
	w := NewWriter(4)
	w.PutU32(v)
	return w.Bytes()
}

// DecodeU32 decodes a uint32 from BSATN bytes.
func DecodeU32(data []byte) (uint32, error) {
	return NewReader(data).GetU32()
}

// EncodeU64 encodes a uint64 to BSATN bytes.
func EncodeU64(v uint64) []byte {
	w := NewWriter(8)
	w.PutU64(v)
	return w.Bytes()
}

// DecodeU64 decodes a uint64 from BSATN bytes.
func DecodeU64(data []byte) (uint64, error) {
	return NewReader(data).GetU64()
}

// EncodeI8 encodes an int8 to BSATN bytes.
func EncodeI8(v int8) []byte {
	w := NewWriter(1)
	w.PutI8(v)
	return w.Bytes()
}

// DecodeI8 decodes an int8 from BSATN bytes.
func DecodeI8(data []byte) (int8, error) {
	return NewReader(data).GetI8()
}

// EncodeI16 encodes an int16 to BSATN bytes.
func EncodeI16(v int16) []byte {
	w := NewWriter(2)
	w.PutI16(v)
	return w.Bytes()
}

// DecodeI16 decodes an int16 from BSATN bytes.
func DecodeI16(data []byte) (int16, error) {
	return NewReader(data).GetI16()
}

// EncodeI32 encodes an int32 to BSATN bytes.
func EncodeI32(v int32) []byte {
	w := NewWriter(4)
	w.PutI32(v)
	return w.Bytes()
}

// DecodeI32 decodes an int32 from BSATN bytes.
func DecodeI32(data []byte) (int32, error) {
	return NewReader(data).GetI32()
}

// EncodeI64 encodes an int64 to BSATN bytes.
func EncodeI64(v int64) []byte {
	w := NewWriter(8)
	w.PutI64(v)
	return w.Bytes()
}

// DecodeI64 decodes an int64 from BSATN bytes.
func DecodeI64(data []byte) (int64, error) {
	return NewReader(data).GetI64()
}

// EncodeF32 encodes a float32 to BSATN bytes.
func EncodeF32(v float32) []byte {
	w := NewWriter(4)
	w.PutF32(v)
	return w.Bytes()
}

// DecodeF32 decodes a float32 from BSATN bytes.
func DecodeF32(data []byte) (float32, error) {
	return NewReader(data).GetF32()
}

// EncodeF64 encodes a float64 to BSATN bytes.
func EncodeF64(v float64) []byte {
	w := NewWriter(8)
	w.PutF64(v)
	return w.Bytes()
}

// DecodeF64 decodes a float64 from BSATN bytes.
func DecodeF64(data []byte) (float64, error) {
	return NewReader(data).GetF64()
}

// EncodeString encodes a string to BSATN bytes (u32 LE length prefix + UTF-8 bytes).
func EncodeString(v string) []byte {
	w := NewWriter(4 + len(v))
	w.PutString(v)
	return w.Bytes()
}

// DecodeString decodes a string from BSATN bytes.
func DecodeString(data []byte) (string, error) {
	return NewReader(data).GetString()
}
