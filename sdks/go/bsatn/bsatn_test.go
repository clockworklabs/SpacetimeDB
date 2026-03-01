package bsatn_test

import (
	"math"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
)

// === BOOL WRITE TESTS ===

func TestWriteBoolTrue(t *testing.T) {
	w := bsatn.NewWriter(1)
	w.PutBool(true)
	assert.Equal(t, []byte{0x01}, w.Bytes())
}

func TestWriteBoolFalse(t *testing.T) {
	w := bsatn.NewWriter(1)
	w.PutBool(false)
	assert.Equal(t, []byte{0x00}, w.Bytes())
}

// === BOOL READ TESTS ===

func TestReadBoolTrue(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01})
	v, err := r.GetBool()
	require.NoError(t, err)
	assert.True(t, v)
}

func TestReadBoolFalse(t *testing.T) {
	r := bsatn.NewReader([]byte{0x00})
	v, err := r.GetBool()
	require.NoError(t, err)
	assert.False(t, v)
}

func TestReadBoolInvalid(t *testing.T) {
	r := bsatn.NewReader([]byte{0x02})
	_, err := r.GetBool()
	require.Error(t, err)
	var invalidBool *bsatn.ErrInvalidBool
	assert.ErrorAs(t, err, &invalidBool)
	assert.Equal(t, uint8(0x02), invalidBool.Value)
}

// === UNSIGNED INTEGER WRITE TESTS ===

func TestWriteU8(t *testing.T) {
	w := bsatn.NewWriter(1)
	w.PutU8(42)
	assert.Equal(t, []byte{0x2a}, w.Bytes())
}

func TestWriteU8Max(t *testing.T) {
	w := bsatn.NewWriter(1)
	w.PutU8(math.MaxUint8)
	assert.Equal(t, []byte{0xff}, w.Bytes())
}

func TestWriteU16(t *testing.T) {
	w := bsatn.NewWriter(2)
	w.PutU16(0x0102)
	assert.Equal(t, []byte{0x02, 0x01}, w.Bytes())
}

func TestWriteU16Max(t *testing.T) {
	w := bsatn.NewWriter(2)
	w.PutU16(math.MaxUint16)
	assert.Equal(t, []byte{0xff, 0xff}, w.Bytes())
}

func TestWriteU32(t *testing.T) {
	w := bsatn.NewWriter(4)
	w.PutU32(42)
	assert.Equal(t, []byte{0x2a, 0x00, 0x00, 0x00}, w.Bytes())
}

func TestWriteU32Max(t *testing.T) {
	w := bsatn.NewWriter(4)
	w.PutU32(math.MaxUint32)
	assert.Equal(t, []byte{0xff, 0xff, 0xff, 0xff}, w.Bytes())
}

func TestWriteU64(t *testing.T) {
	w := bsatn.NewWriter(8)
	w.PutU64(42)
	assert.Equal(t, []byte{0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00}, w.Bytes())
}

func TestWriteU64Max(t *testing.T) {
	w := bsatn.NewWriter(8)
	w.PutU64(math.MaxUint64)
	assert.Equal(t, []byte{0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff}, w.Bytes())
}

// === SIGNED INTEGER WRITE TESTS ===

func TestWriteI8Positive(t *testing.T) {
	w := bsatn.NewWriter(1)
	w.PutI8(42)
	assert.Equal(t, []byte{0x2a}, w.Bytes())
}

func TestWriteI8Negative(t *testing.T) {
	w := bsatn.NewWriter(1)
	w.PutI8(-1)
	assert.Equal(t, []byte{0xff}, w.Bytes())
}

func TestWriteI8MinMax(t *testing.T) {
	w := bsatn.NewWriter(2)
	w.PutI8(math.MinInt8)
	w.PutI8(math.MaxInt8)
	assert.Equal(t, []byte{0x80, 0x7f}, w.Bytes())
}

func TestWriteI16Negative(t *testing.T) {
	w := bsatn.NewWriter(2)
	w.PutI16(-1)
	assert.Equal(t, []byte{0xff, 0xff}, w.Bytes())
}

func TestWriteI16MinMax(t *testing.T) {
	w := bsatn.NewWriter(4)
	w.PutI16(math.MinInt16)
	w.PutI16(math.MaxInt16)
	// MinInt16 = -32768 = 0x8000 LE: {0x00, 0x80}
	// MaxInt16 = 32767 = 0x7FFF LE: {0xff, 0x7f}
	assert.Equal(t, []byte{0x00, 0x80, 0xff, 0x7f}, w.Bytes())
}

func TestWriteI32Negative(t *testing.T) {
	w := bsatn.NewWriter(4)
	w.PutI32(-1)
	assert.Equal(t, []byte{0xff, 0xff, 0xff, 0xff}, w.Bytes())
}

func TestWriteI32Value(t *testing.T) {
	w := bsatn.NewWriter(4)
	w.PutI32(0x01020304)
	assert.Equal(t, []byte{0x04, 0x03, 0x02, 0x01}, w.Bytes())
}

func TestWriteI64Negative(t *testing.T) {
	w := bsatn.NewWriter(8)
	w.PutI64(-1)
	assert.Equal(t, []byte{0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff}, w.Bytes())
}

func TestWriteI64Value(t *testing.T) {
	w := bsatn.NewWriter(8)
	w.PutI64(0x0102030405060708)
	assert.Equal(t, []byte{0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01}, w.Bytes())
}

// === FLOAT WRITE TESTS ===

func TestWriteF32One(t *testing.T) {
	w := bsatn.NewWriter(4)
	w.PutF32(1.0)
	// IEEE 754: 1.0 = 0x3F800000 LE: {0x00, 0x00, 0x80, 0x3f}
	assert.Equal(t, []byte{0x00, 0x00, 0x80, 0x3f}, w.Bytes())
}

func TestWriteF32NegativeZero(t *testing.T) {
	w := bsatn.NewWriter(4)
	w.PutF32(float32(math.Copysign(0, -1)))
	// IEEE 754: -0.0 = 0x80000000 LE: {0x00, 0x00, 0x00, 0x80}
	assert.Equal(t, []byte{0x00, 0x00, 0x00, 0x80}, w.Bytes())
}

func TestWriteF32Pi(t *testing.T) {
	w := bsatn.NewWriter(4)
	w.PutF32(math.Pi)
	// math.Pi as float32 = 0x40490FDB LE: {0xdb, 0x0f, 0x49, 0x40}
	assert.Equal(t, []byte{0xdb, 0x0f, 0x49, 0x40}, w.Bytes())
}

func TestWriteF64One(t *testing.T) {
	w := bsatn.NewWriter(8)
	w.PutF64(1.0)
	// IEEE 754: 1.0 = 0x3FF0000000000000 LE
	assert.Equal(t, []byte{0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f}, w.Bytes())
}

func TestWriteF64Pi(t *testing.T) {
	w := bsatn.NewWriter(8)
	w.PutF64(math.Pi)
	// IEEE 754: pi = 0x400921FB54442D18 LE
	assert.Equal(t, []byte{0x18, 0x2d, 0x44, 0x54, 0xfb, 0x21, 0x09, 0x40}, w.Bytes())
}

// === UNSIGNED INTEGER READ TESTS ===

func TestReadU8(t *testing.T) {
	r := bsatn.NewReader([]byte{0x2a})
	v, err := r.GetU8()
	require.NoError(t, err)
	assert.Equal(t, uint8(42), v)
}

func TestReadU16(t *testing.T) {
	r := bsatn.NewReader([]byte{0x02, 0x01})
	v, err := r.GetU16()
	require.NoError(t, err)
	assert.Equal(t, uint16(0x0102), v)
}

func TestReadU32(t *testing.T) {
	r := bsatn.NewReader([]byte{0x2a, 0x00, 0x00, 0x00})
	v, err := r.GetU32()
	require.NoError(t, err)
	assert.Equal(t, uint32(42), v)
}

func TestReadU64(t *testing.T) {
	r := bsatn.NewReader([]byte{0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00})
	v, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(42), v)
}

// === SIGNED INTEGER READ TESTS ===

func TestReadI8Negative(t *testing.T) {
	r := bsatn.NewReader([]byte{0xff})
	v, err := r.GetI8()
	require.NoError(t, err)
	assert.Equal(t, int8(-1), v)
}

func TestReadI16Negative(t *testing.T) {
	r := bsatn.NewReader([]byte{0xff, 0xff})
	v, err := r.GetI16()
	require.NoError(t, err)
	assert.Equal(t, int16(-1), v)
}

func TestReadI32Negative(t *testing.T) {
	r := bsatn.NewReader([]byte{0xff, 0xff, 0xff, 0xff})
	v, err := r.GetI32()
	require.NoError(t, err)
	assert.Equal(t, int32(-1), v)
}

func TestReadI64Negative(t *testing.T) {
	r := bsatn.NewReader([]byte{0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff})
	v, err := r.GetI64()
	require.NoError(t, err)
	assert.Equal(t, int64(-1), v)
}

// === FLOAT READ TESTS ===

func TestReadF32(t *testing.T) {
	r := bsatn.NewReader([]byte{0x00, 0x00, 0x80, 0x3f})
	v, err := r.GetF32()
	require.NoError(t, err)
	assert.Equal(t, float32(1.0), v)
}

func TestReadF64(t *testing.T) {
	r := bsatn.NewReader([]byte{0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f})
	v, err := r.GetF64()
	require.NoError(t, err)
	assert.Equal(t, float64(1.0), v)
}

// === STRING TESTS ===

func TestWriteString(t *testing.T) {
	w := bsatn.NewWriter(16)
	w.PutString("hello")
	expected := []byte{0x05, 0x00, 0x00, 0x00, 'h', 'e', 'l', 'l', 'o'}
	assert.Equal(t, expected, w.Bytes())
}

func TestWriteEmptyString(t *testing.T) {
	w := bsatn.NewWriter(4)
	w.PutString("")
	assert.Equal(t, []byte{0x00, 0x00, 0x00, 0x00}, w.Bytes())
}

func TestWriteStringUTF8(t *testing.T) {
	w := bsatn.NewWriter(16)
	// U+00E9 (e with acute) is 0xC3 0xA9 in UTF-8 (2 bytes)
	w.PutString("\u00e9")
	// length prefix is byte count, not rune count
	expected := []byte{0x02, 0x00, 0x00, 0x00, 0xc3, 0xa9}
	assert.Equal(t, expected, w.Bytes())
}

func TestWriteStringMultiByteUTF8(t *testing.T) {
	w := bsatn.NewWriter(16)
	// U+1F600 (grinning face) is 0xF0 0x9F 0x98 0x80 in UTF-8 (4 bytes)
	w.PutString("\U0001F600")
	expected := []byte{0x04, 0x00, 0x00, 0x00, 0xf0, 0x9f, 0x98, 0x80}
	assert.Equal(t, expected, w.Bytes())
}

func TestReadString(t *testing.T) {
	data := []byte{0x05, 0x00, 0x00, 0x00, 'h', 'e', 'l', 'l', 'o'}
	r := bsatn.NewReader(data)
	v, err := r.GetString()
	require.NoError(t, err)
	assert.Equal(t, "hello", v)
}

func TestReadEmptyString(t *testing.T) {
	data := []byte{0x00, 0x00, 0x00, 0x00}
	r := bsatn.NewReader(data)
	v, err := r.GetString()
	require.NoError(t, err)
	assert.Equal(t, "", v)
}

func TestReadStringUTF8(t *testing.T) {
	data := []byte{0x02, 0x00, 0x00, 0x00, 0xc3, 0xa9}
	r := bsatn.NewReader(data)
	v, err := r.GetString()
	require.NoError(t, err)
	assert.Equal(t, "\u00e9", v)
}

// === ROUND-TRIP TESTS ===

func TestRoundTripBool(t *testing.T) {
	for _, tc := range []bool{true, false} {
		encoded := bsatn.EncodeBool(tc)
		decoded, err := bsatn.DecodeBool(encoded)
		require.NoError(t, err)
		assert.Equal(t, tc, decoded)
	}
}

func TestRoundTripU8(t *testing.T) {
	for _, tc := range []uint8{0, 1, 42, math.MaxUint8} {
		encoded := bsatn.EncodeU8(tc)
		decoded, err := bsatn.DecodeU8(encoded)
		require.NoError(t, err)
		assert.Equal(t, tc, decoded)
	}
}

func TestRoundTripU16(t *testing.T) {
	for _, tc := range []uint16{0, 1, 0x0102, math.MaxUint16} {
		encoded := bsatn.EncodeU16(tc)
		decoded, err := bsatn.DecodeU16(encoded)
		require.NoError(t, err)
		assert.Equal(t, tc, decoded)
	}
}

func TestRoundTripU32(t *testing.T) {
	for _, tc := range []uint32{0, 1, 42, 0x01020304, math.MaxUint32} {
		encoded := bsatn.EncodeU32(tc)
		decoded, err := bsatn.DecodeU32(encoded)
		require.NoError(t, err)
		assert.Equal(t, tc, decoded)
	}
}

func TestRoundTripU64(t *testing.T) {
	for _, tc := range []uint64{0, 1, 42, 0x0102030405060708, math.MaxUint64} {
		encoded := bsatn.EncodeU64(tc)
		decoded, err := bsatn.DecodeU64(encoded)
		require.NoError(t, err)
		assert.Equal(t, tc, decoded)
	}
}

func TestRoundTripI8(t *testing.T) {
	for _, tc := range []int8{math.MinInt8, -1, 0, 1, math.MaxInt8} {
		encoded := bsatn.EncodeI8(tc)
		decoded, err := bsatn.DecodeI8(encoded)
		require.NoError(t, err)
		assert.Equal(t, tc, decoded)
	}
}

func TestRoundTripI16(t *testing.T) {
	for _, tc := range []int16{math.MinInt16, -1, 0, 1, math.MaxInt16} {
		encoded := bsatn.EncodeI16(tc)
		decoded, err := bsatn.DecodeI16(encoded)
		require.NoError(t, err)
		assert.Equal(t, tc, decoded)
	}
}

func TestRoundTripI32(t *testing.T) {
	for _, tc := range []int32{math.MinInt32, -1, 0, 1, math.MaxInt32} {
		encoded := bsatn.EncodeI32(tc)
		decoded, err := bsatn.DecodeI32(encoded)
		require.NoError(t, err)
		assert.Equal(t, tc, decoded)
	}
}

func TestRoundTripI64(t *testing.T) {
	for _, tc := range []int64{math.MinInt64, -1, 0, 1, math.MaxInt64} {
		encoded := bsatn.EncodeI64(tc)
		decoded, err := bsatn.DecodeI64(encoded)
		require.NoError(t, err)
		assert.Equal(t, tc, decoded)
	}
}

func TestRoundTripF32(t *testing.T) {
	for _, tc := range []float32{0, 1.0, -1.0, math.SmallestNonzeroFloat32, math.MaxFloat32, float32(math.Pi)} {
		encoded := bsatn.EncodeF32(tc)
		decoded, err := bsatn.DecodeF32(encoded)
		require.NoError(t, err)
		assert.Equal(t, tc, decoded)
	}
}

func TestRoundTripF64(t *testing.T) {
	for _, tc := range []float64{0, 1.0, -1.0, math.SmallestNonzeroFloat64, math.MaxFloat64, math.Pi} {
		encoded := bsatn.EncodeF64(tc)
		decoded, err := bsatn.DecodeF64(encoded)
		require.NoError(t, err)
		assert.Equal(t, tc, decoded)
	}
}

func TestRoundTripString(t *testing.T) {
	for _, tc := range []string{"", "hello", "\u00e9", "\U0001F600", "hello world"} {
		encoded := bsatn.EncodeString(tc)
		decoded, err := bsatn.DecodeString(encoded)
		require.NoError(t, err)
		assert.Equal(t, tc, decoded)
	}
}

// === CONVENIENCE FUNCTION TESTS ===

func TestEncodeBoolBytes(t *testing.T) {
	assert.Equal(t, []byte{0x01}, bsatn.EncodeBool(true))
	assert.Equal(t, []byte{0x00}, bsatn.EncodeBool(false))
}

func TestEncodeU32Bytes(t *testing.T) {
	assert.Equal(t, []byte{0x2a, 0x00, 0x00, 0x00}, bsatn.EncodeU32(42))
}

func TestEncodeI64Bytes(t *testing.T) {
	assert.Equal(t, []byte{0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff}, bsatn.EncodeI64(-1))
}

func TestEncodeStringBytes(t *testing.T) {
	expected := []byte{0x05, 0x00, 0x00, 0x00, 'h', 'e', 'l', 'l', 'o'}
	assert.Equal(t, expected, bsatn.EncodeString("hello"))
}

// === ARRAY TESTS ===

func TestWriteByteArray(t *testing.T) {
	w := bsatn.NewWriter(16)
	bsatn.WriteByteArray(w, []byte{0xDE, 0xAD, 0xBE, 0xEF})
	// u32 LE length (4) + raw bytes
	expected := []byte{0x04, 0x00, 0x00, 0x00, 0xDE, 0xAD, 0xBE, 0xEF}
	assert.Equal(t, expected, w.Bytes())
}

func TestWriteByteArrayEmpty(t *testing.T) {
	w := bsatn.NewWriter(4)
	bsatn.WriteByteArray(w, []byte{})
	assert.Equal(t, []byte{0x00, 0x00, 0x00, 0x00}, w.Bytes())
}

func TestReadByteArray(t *testing.T) {
	data := []byte{0x04, 0x00, 0x00, 0x00, 0xDE, 0xAD, 0xBE, 0xEF}
	r := bsatn.NewReader(data)
	v, err := bsatn.ReadByteArray(r)
	require.NoError(t, err)
	assert.Equal(t, []byte{0xDE, 0xAD, 0xBE, 0xEF}, v)
}

func TestReadByteArrayEmpty(t *testing.T) {
	data := []byte{0x00, 0x00, 0x00, 0x00}
	r := bsatn.NewReader(data)
	v, err := bsatn.ReadByteArray(r)
	require.NoError(t, err)
	assert.Empty(t, v)
}

func TestReadArrayOfU32(t *testing.T) {
	// Array of 3 u32 values: [1, 2, 3]
	data := []byte{
		0x03, 0x00, 0x00, 0x00, // count = 3
		0x01, 0x00, 0x00, 0x00, // 1
		0x02, 0x00, 0x00, 0x00, // 2
		0x03, 0x00, 0x00, 0x00, // 3
	}
	r := bsatn.NewReader(data)
	v, err := bsatn.ReadArray(r, func(r bsatn.Reader) (uint32, error) {
		return r.GetU32()
	})
	require.NoError(t, err)
	assert.Equal(t, []uint32{1, 2, 3}, v)
}

func TestReadArrayEmpty(t *testing.T) {
	data := []byte{0x00, 0x00, 0x00, 0x00}
	r := bsatn.NewReader(data)
	v, err := bsatn.ReadArray(r, func(r bsatn.Reader) (uint32, error) {
		return r.GetU32()
	})
	require.NoError(t, err)
	assert.Empty(t, v)
}

// === OPTION TESTS ===

func TestReadOptionSome(t *testing.T) {
	// Option<u32> Some(42): tag 0 + u32 LE 42
	data := []byte{0x00, 0x2a, 0x00, 0x00, 0x00}
	r := bsatn.NewReader(data)
	v, err := bsatn.ReadOption(r, func(r bsatn.Reader) (uint32, error) {
		return r.GetU32()
	})
	require.NoError(t, err)
	require.NotNil(t, v)
	assert.Equal(t, uint32(42), *v)
}

func TestReadOptionNone(t *testing.T) {
	// Option<u32> None: tag 1
	data := []byte{0x01}
	r := bsatn.NewReader(data)
	v, err := bsatn.ReadOption(r, func(r bsatn.Reader) (uint32, error) {
		return r.GetU32()
	})
	require.NoError(t, err)
	assert.Nil(t, v)
}

func TestReadOptionInvalidTag(t *testing.T) {
	// Option with invalid tag 5
	data := []byte{0x05}
	r := bsatn.NewReader(data)
	_, err := bsatn.ReadOption(r, func(r bsatn.Reader) (uint32, error) {
		return r.GetU32()
	})
	require.Error(t, err)
	var invalidTag *bsatn.ErrInvalidTag
	assert.ErrorAs(t, err, &invalidTag)
	assert.Equal(t, uint8(5), invalidTag.Tag)
	assert.Equal(t, "Option", invalidTag.SumName)
}

// === SUM TAG TESTS ===

func TestWriteSumTag(t *testing.T) {
	w := bsatn.NewWriter(1)
	w.PutSumTag(3)
	assert.Equal(t, []byte{0x03}, w.Bytes())
}

func TestReadSumTag(t *testing.T) {
	r := bsatn.NewReader([]byte{0x03})
	v, err := r.GetSumTag()
	require.NoError(t, err)
	assert.Equal(t, uint8(3), v)
}

// === MAP TESTS ===

func TestReadMap(t *testing.T) {
	// Map with 2 entries: {"a": 1, "b": 2}
	data := []byte{
		0x02, 0x00, 0x00, 0x00, // count = 2
		// key "a"
		0x01, 0x00, 0x00, 0x00, 'a',
		// value 1
		0x01, 0x00, 0x00, 0x00,
		// key "b"
		0x01, 0x00, 0x00, 0x00, 'b',
		// value 2
		0x02, 0x00, 0x00, 0x00,
	}
	r := bsatn.NewReader(data)
	m, err := bsatn.ReadMap(r,
		func(r bsatn.Reader) (string, error) { return r.GetString() },
		func(r bsatn.Reader) (uint32, error) { return r.GetU32() },
	)
	require.NoError(t, err)
	assert.Len(t, m, 2)
	assert.Equal(t, uint32(1), m["a"])
	assert.Equal(t, uint32(2), m["b"])
}

func TestReadMapEmpty(t *testing.T) {
	data := []byte{0x00, 0x00, 0x00, 0x00}
	r := bsatn.NewReader(data)
	m, err := bsatn.ReadMap(r,
		func(r bsatn.Reader) (string, error) { return r.GetString() },
		func(r bsatn.Reader) (uint32, error) { return r.GetU32() },
	)
	require.NoError(t, err)
	assert.Empty(t, m)
}

// === ARRAY/MAP LENGTH PREFIX TESTS ===

func TestWriteArrayLen(t *testing.T) {
	w := bsatn.NewWriter(4)
	w.PutArrayLen(5)
	assert.Equal(t, []byte{0x05, 0x00, 0x00, 0x00}, w.Bytes())
}

func TestReadArrayLen(t *testing.T) {
	r := bsatn.NewReader([]byte{0x05, 0x00, 0x00, 0x00})
	v, err := r.GetArrayLen()
	require.NoError(t, err)
	assert.Equal(t, uint32(5), v)
}

func TestWriteMapLen(t *testing.T) {
	w := bsatn.NewWriter(4)
	w.PutMapLen(10)
	assert.Equal(t, []byte{0x0a, 0x00, 0x00, 0x00}, w.Bytes())
}

func TestReadMapLen(t *testing.T) {
	r := bsatn.NewReader([]byte{0x0a, 0x00, 0x00, 0x00})
	v, err := r.GetMapLen()
	require.NoError(t, err)
	assert.Equal(t, uint32(10), v)
}

// === ERROR CASES ===

func TestReadU8EmptyBuffer(t *testing.T) {
	r := bsatn.NewReader([]byte{})
	_, err := r.GetU8()
	require.Error(t, err)
	var bufErr *bsatn.ErrBufferTooShort
	assert.ErrorAs(t, err, &bufErr)
	assert.Equal(t, 1, bufErr.Expected)
	assert.Equal(t, 0, bufErr.Given)
}

func TestReadU16BufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01})
	_, err := r.GetU16()
	require.Error(t, err)
	var bufErr *bsatn.ErrBufferTooShort
	assert.ErrorAs(t, err, &bufErr)
	assert.Equal(t, 2, bufErr.Expected)
	assert.Equal(t, 1, bufErr.Given)
}

func TestReadU32BufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01, 0x02})
	_, err := r.GetU32()
	require.Error(t, err)
	var bufErr *bsatn.ErrBufferTooShort
	assert.ErrorAs(t, err, &bufErr)
	assert.Equal(t, 4, bufErr.Expected)
	assert.Equal(t, 2, bufErr.Given)
}

func TestReadU64BufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01, 0x02, 0x03, 0x04})
	_, err := r.GetU64()
	require.Error(t, err)
	var bufErr *bsatn.ErrBufferTooShort
	assert.ErrorAs(t, err, &bufErr)
	assert.Equal(t, 8, bufErr.Expected)
	assert.Equal(t, 4, bufErr.Given)
}

func TestReadBoolBufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{})
	_, err := r.GetBool()
	require.Error(t, err)
	var bufErr *bsatn.ErrBufferTooShort
	assert.ErrorAs(t, err, &bufErr)
}

func TestReadStringBufferTooShortForLength(t *testing.T) {
	// Only 2 bytes, but u32 length prefix requires 4
	r := bsatn.NewReader([]byte{0x05, 0x00})
	_, err := r.GetString()
	require.Error(t, err)
}

func TestReadStringBufferTooShortForPayload(t *testing.T) {
	// Length says 5 bytes, but only 2 available
	r := bsatn.NewReader([]byte{0x05, 0x00, 0x00, 0x00, 'h', 'i'})
	_, err := r.GetString()
	require.Error(t, err)
}

func TestReadByteArrayBufferTooShort(t *testing.T) {
	// Length says 10 bytes, but only 3 available
	r := bsatn.NewReader([]byte{0x0a, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03})
	_, err := bsatn.ReadByteArray(r)
	require.Error(t, err)
}

// === REMAINING TESTS ===

func TestReaderRemaining(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01, 0x02, 0x03, 0x04})
	assert.Equal(t, 4, r.Remaining())

	_, err := r.GetU8()
	require.NoError(t, err)
	assert.Equal(t, 3, r.Remaining())

	_, err = r.GetU16()
	require.NoError(t, err)
	assert.Equal(t, 1, r.Remaining())
}

// === SEQUENTIAL READ/WRITE (PRODUCT PATTERN) ===

func TestSequentialWriteMultipleValues(t *testing.T) {
	// Simulate a product type with fields: bool, u32, string
	w := bsatn.NewWriter(32)
	w.PutBool(true)
	w.PutU32(42)
	w.PutString("test")

	expected := []byte{
		0x01,                         // bool true
		0x2a, 0x00, 0x00, 0x00,      // u32 42
		0x04, 0x00, 0x00, 0x00,      // string length 4
		't', 'e', 's', 't',          // string "test"
	}
	assert.Equal(t, expected, w.Bytes())
}

func TestSequentialReadMultipleValues(t *testing.T) {
	data := []byte{
		0x01,                         // bool true
		0x2a, 0x00, 0x00, 0x00,      // u32 42
		0x04, 0x00, 0x00, 0x00,      // string length 4
		't', 'e', 's', 't',          // string "test"
	}
	r := bsatn.NewReader(data)

	b, err := r.GetBool()
	require.NoError(t, err)
	assert.True(t, b)

	u, err := r.GetU32()
	require.NoError(t, err)
	assert.Equal(t, uint32(42), u)

	s, err := r.GetString()
	require.NoError(t, err)
	assert.Equal(t, "test", s)

	assert.Equal(t, 0, r.Remaining())
}

// === ENCODE/DECODE GENERIC HELPERS ===

func TestEncodeDecodeGeneric(t *testing.T) {
	data := bsatn.EncodeU32(12345)
	v, err := bsatn.Decode(data, func(r bsatn.Reader) (uint32, error) {
		return r.GetU32()
	})
	require.NoError(t, err)
	assert.Equal(t, uint32(12345), v)
}

// === PUTBYTES RAW TEST ===

func TestPutBytesRaw(t *testing.T) {
	w := bsatn.NewWriter(4)
	w.PutBytes([]byte{0xCA, 0xFE, 0xBA, 0xBE})
	assert.Equal(t, []byte{0xCA, 0xFE, 0xBA, 0xBE}, w.Bytes())
}

// === ERROR MESSAGE FORMAT TESTS ===

func TestBufferTooShortErrorMessage(t *testing.T) {
	err := &bsatn.ErrBufferTooShort{ForType: "u32", Expected: 4, Given: 2}
	assert.Contains(t, err.Error(), "u32")
	assert.Contains(t, err.Error(), "4")
	assert.Contains(t, err.Error(), "2")
}

func TestInvalidBoolErrorMessage(t *testing.T) {
	err := &bsatn.ErrInvalidBool{Value: 0x42}
	assert.Contains(t, err.Error(), "0x42")
}

func TestInvalidTagErrorMessage(t *testing.T) {
	err := &bsatn.ErrInvalidTag{Tag: 5, SumName: "MyEnum"}
	assert.Contains(t, err.Error(), "5")
	assert.Contains(t, err.Error(), "MyEnum")
}

// === GETBYTES TEST ===

func TestGetBytes(t *testing.T) {
	data := []byte{0x01, 0x02, 0x03, 0x04, 0x05}
	r := bsatn.NewReader(data)
	b, err := r.GetBytes(3)
	require.NoError(t, err)
	assert.Equal(t, []byte{0x01, 0x02, 0x03}, b)
	assert.Equal(t, 2, r.Remaining())
}

func TestGetBytesTooFew(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01})
	_, err := r.GetBytes(5)
	require.Error(t, err)
}

// === F32/F64 SPECIAL VALUES ===

func TestRoundTripF32Inf(t *testing.T) {
	posInf := float32(math.Inf(1))
	encoded := bsatn.EncodeF32(posInf)
	decoded, err := bsatn.DecodeF32(encoded)
	require.NoError(t, err)
	assert.True(t, math.IsInf(float64(decoded), 1))

	negInf := float32(math.Inf(-1))
	encoded = bsatn.EncodeF32(negInf)
	decoded, err = bsatn.DecodeF32(encoded)
	require.NoError(t, err)
	assert.True(t, math.IsInf(float64(decoded), -1))
}

func TestRoundTripF64Inf(t *testing.T) {
	posInf := math.Inf(1)
	encoded := bsatn.EncodeF64(posInf)
	decoded, err := bsatn.DecodeF64(encoded)
	require.NoError(t, err)
	assert.True(t, math.IsInf(decoded, 1))

	negInf := math.Inf(-1)
	encoded = bsatn.EncodeF64(negInf)
	decoded, err = bsatn.DecodeF64(encoded)
	require.NoError(t, err)
	assert.True(t, math.IsInf(decoded, -1))
}

func TestRoundTripF32NaN(t *testing.T) {
	nan := float32(math.NaN())
	encoded := bsatn.EncodeF32(nan)
	decoded, err := bsatn.DecodeF32(encoded)
	require.NoError(t, err)
	assert.True(t, math.IsNaN(float64(decoded)))
}

func TestRoundTripF64NaN(t *testing.T) {
	nan := math.NaN()
	encoded := bsatn.EncodeF64(nan)
	decoded, err := bsatn.DecodeF64(encoded)
	require.NoError(t, err)
	assert.True(t, math.IsNaN(decoded))
}

// === WRITER CAPACITY ===

func TestWriterGrowsBeyondInitialCapacity(t *testing.T) {
	w := bsatn.NewWriter(1) // tiny initial capacity
	w.PutU64(math.MaxUint64)
	assert.Len(t, w.Bytes(), 8)
	assert.Equal(t, []byte{0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff}, w.Bytes())
}
