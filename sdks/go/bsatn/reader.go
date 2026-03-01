package bsatn

import (
	"encoding/binary"
	"math"
	"unsafe"
)

// NewReader creates a new BSATN reader from the given byte slice.
func NewReader(data []byte) Reader {
	return &reader{data: data}
}

type reader struct {
	data []byte
	pos  int
}

func (r *reader) readBytes(n int, forType string) ([]byte, error) {
	remaining := len(r.data) - r.pos
	if remaining < n {
		return nil, &ErrBufferTooShort{
			ForType:  forType,
			Expected: n,
			Given:    remaining,
		}
	}
	b := r.data[r.pos : r.pos+n]
	r.pos += n
	return b, nil
}

func (r *reader) GetBool() (bool, error) {
	b, err := r.readBytes(1, "bool")
	if err != nil {
		return false, err
	}
	switch b[0] {
	case 0x00:
		return false, nil
	case 0x01:
		return true, nil
	default:
		return false, &ErrInvalidBool{Value: b[0]}
	}
}

func (r *reader) GetU8() (uint8, error) {
	b, err := r.readBytes(1, "u8")
	if err != nil {
		return 0, err
	}
	return b[0], nil
}

func (r *reader) GetU16() (uint16, error) {
	b, err := r.readBytes(2, "u16")
	if err != nil {
		return 0, err
	}
	return binary.LittleEndian.Uint16(b), nil
}

func (r *reader) GetU32() (uint32, error) {
	b, err := r.readBytes(4, "u32")
	if err != nil {
		return 0, err
	}
	return binary.LittleEndian.Uint32(b), nil
}

func (r *reader) GetU64() (uint64, error) {
	b, err := r.readBytes(8, "u64")
	if err != nil {
		return 0, err
	}
	return binary.LittleEndian.Uint64(b), nil
}

func (r *reader) GetI8() (int8, error) {
	b, err := r.readBytes(1, "i8")
	if err != nil {
		return 0, err
	}
	return int8(b[0]), nil
}

func (r *reader) GetI16() (int16, error) {
	v, err := r.GetU16()
	if err != nil {
		return 0, err
	}
	return int16(v), nil
}

func (r *reader) GetI32() (int32, error) {
	v, err := r.GetU32()
	if err != nil {
		return 0, err
	}
	return int32(v), nil
}

func (r *reader) GetI64() (int64, error) {
	v, err := r.GetU64()
	if err != nil {
		return 0, err
	}
	return int64(v), nil
}

func (r *reader) GetF32() (float32, error) {
	v, err := r.GetU32()
	if err != nil {
		return 0, err
	}
	return math.Float32frombits(v), nil
}

func (r *reader) GetF64() (float64, error) {
	v, err := r.GetU64()
	if err != nil {
		return 0, err
	}
	return math.Float64frombits(v), nil
}

func (r *reader) GetString() (string, error) {
	length, err := r.GetU32()
	if err != nil {
		return "", err
	}
	b, err := r.GetBytes(int(length))
	if err != nil {
		return "", err
	}
	return string(b), nil
}

func (r *reader) GetBytes(n int) ([]byte, error) {
	return r.readBytes(n, "bytes")
}

func (r *reader) GetArrayLen() (uint32, error) {
	return r.GetU32()
}

func (r *reader) GetMapLen() (uint32, error) {
	return r.GetU32()
}

func (r *reader) GetSumTag() (uint8, error) {
	return r.GetU8()
}

func (r *reader) Remaining() int {
	return len(r.data) - r.pos
}

// NewZeroCopyReader creates a Reader where GetString() returns strings
// that share the underlying buffer memory (no copy).
// SAFETY: The data buffer must outlive all strings decoded from it.
// As long as any decoded string is reachable, the GC keeps data alive.
func NewZeroCopyReader(data []byte) Reader {
	return &zeroCopyReader{reader: reader{data: data}}
}

type zeroCopyReader struct {
	reader
}

func (r *zeroCopyReader) GetString() (string, error) {
	length, err := r.GetU32()
	if err != nil {
		return "", err
	}
	if length == 0 {
		return "", nil
	}
	b, err := r.GetBytes(int(length))
	if err != nil {
		return "", err
	}
	return unsafe.String(&b[0], len(b)), nil
}
