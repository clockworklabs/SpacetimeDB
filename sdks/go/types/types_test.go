package types_test

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// === UINT128 TESTS ===

func TestUint128RoundTrip(t *testing.T) {
	original := types.NewUint128(0x0102030405060708, 0x090A0B0C0D0E0F10)
	encoded := bsatn.Encode(original)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadUint128(r)
	require.NoError(t, err)
	assert.Equal(t, original.Bytes(), decoded.Bytes())
	assert.Equal(t, original.Lo(), decoded.Lo())
	assert.Equal(t, original.Hi(), decoded.Hi())
}

func TestUint128ExactBytes(t *testing.T) {
	u := types.NewUint128(1, 0)
	encoded := bsatn.Encode(u)
	// lo=1 in LE, hi=0 in LE -> 16 bytes
	expected := []byte{
		0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // lo = 1
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // hi = 0
	}
	assert.Equal(t, expected, encoded)
}

func TestUint128MaxValue(t *testing.T) {
	u := types.NewUint128(0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF)
	encoded := bsatn.Encode(u)
	expected := make([]byte, 16)
	for i := range expected {
		expected[i] = 0xFF
	}
	assert.Equal(t, expected, encoded)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadUint128(r)
	require.NoError(t, err)
	assert.Equal(t, uint64(0xFFFFFFFFFFFFFFFF), decoded.Lo())
	assert.Equal(t, uint64(0xFFFFFFFFFFFFFFFF), decoded.Hi())
}

func TestUint128Zero(t *testing.T) {
	u := types.NewUint128(0, 0)
	assert.True(t, u.IsZero())

	nonZero := types.NewUint128(1, 0)
	assert.False(t, nonZero.IsZero())
}

func TestUint128FromBytes(t *testing.T) {
	var b [16]byte
	b[0] = 0x42
	b[15] = 0xFF
	u := types.NewUint128FromBytes(b)
	assert.Equal(t, b, u.Bytes())
}

func TestUint128String(t *testing.T) {
	u := types.NewUint128(1, 0)
	assert.Equal(t, "1", u.String())

	u2 := types.NewUint128(0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF)
	assert.Equal(t, "340282366920938463463374607431768211455", u2.String())
}

func TestUint128ReadBufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01, 0x02, 0x03})
	_, err := types.ReadUint128(r)
	require.Error(t, err)
}

// === INT128 TESTS ===

func TestInt128RoundTrip(t *testing.T) {
	original := types.NewInt128(0x0102030405060708, 0x090A0B0C0D0E0F10)
	encoded := bsatn.Encode(original)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadInt128(r)
	require.NoError(t, err)
	assert.Equal(t, original.Bytes(), decoded.Bytes())
	assert.Equal(t, original.Lo(), decoded.Lo())
	assert.Equal(t, original.Hi(), decoded.Hi())
}

func TestInt128ExactBytes(t *testing.T) {
	// -1 in two's complement: all 0xFF bytes
	i := types.NewInt128(0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF)
	encoded := bsatn.Encode(i)
	expected := make([]byte, 16)
	for idx := range expected {
		expected[idx] = 0xFF
	}
	assert.Equal(t, expected, encoded)
}

func TestInt128Zero(t *testing.T) {
	i := types.NewInt128(0, 0)
	assert.True(t, i.IsZero())

	nonZero := types.NewInt128(1, 0)
	assert.False(t, nonZero.IsZero())
}

func TestInt128FromBytes(t *testing.T) {
	var b [16]byte
	b[0] = 0x42
	b[15] = 0x80 // sign bit set
	i := types.NewInt128FromBytes(b)
	assert.Equal(t, b, i.Bytes())
}

func TestInt128String(t *testing.T) {
	i := types.NewInt128(1, 0)
	assert.Equal(t, "1", i.String())

	// -1 in two's complement
	neg := types.NewInt128(0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF)
	assert.Equal(t, "-1", neg.String())
}

func TestInt128ReadBufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01})
	_, err := types.ReadInt128(r)
	require.Error(t, err)
}

// === UINT256 TESTS ===

func TestUint256RoundTrip(t *testing.T) {
	var b [32]byte
	for i := range b {
		b[i] = byte(i)
	}
	original := types.NewUint256(b)
	encoded := bsatn.Encode(original)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadUint256(r)
	require.NoError(t, err)
	assert.Equal(t, original.Bytes(), decoded.Bytes())
}

func TestUint256ExactBytes(t *testing.T) {
	u := types.NewUint256FromU64s(1, 0, 0, 0)
	encoded := bsatn.Encode(u)
	expected := []byte{
		0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // a = 1
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // b = 0
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // c = 0
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // d = 0
	}
	assert.Equal(t, expected, encoded)
}

func TestUint256MaxValue(t *testing.T) {
	u := types.NewUint256FromU64s(0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF)
	encoded := bsatn.Encode(u)
	expected := make([]byte, 32)
	for i := range expected {
		expected[i] = 0xFF
	}
	assert.Equal(t, expected, encoded)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadUint256(r)
	require.NoError(t, err)
	assert.Equal(t, u.Bytes(), decoded.Bytes())
}

func TestUint256Zero(t *testing.T) {
	var zero [32]byte
	u := types.NewUint256(zero)
	assert.True(t, u.IsZero())

	nonZero := types.NewUint256FromU64s(1, 0, 0, 0)
	assert.False(t, nonZero.IsZero())
}

func TestUint256FromU64s(t *testing.T) {
	u := types.NewUint256FromU64s(0x0102030405060708, 0x090A0B0C0D0E0F10, 0x1112131415161718, 0x191A1B1C1D1E1F20)
	b := u.Bytes()
	r := bsatn.NewReader(b[:])

	v0, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(0x0102030405060708), v0)

	v1, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(0x090A0B0C0D0E0F10), v1)

	v2, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(0x1112131415161718), v2)

	v3, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(0x191A1B1C1D1E1F20), v3)
}

func TestUint256String(t *testing.T) {
	u := types.NewUint256FromU64s(1, 0, 0, 0)
	assert.Equal(t, "1", u.String())
}

func TestUint256ReadBufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01, 0x02, 0x03})
	_, err := types.ReadUint256(r)
	require.Error(t, err)
}

// === INT256 TESTS ===

func TestInt256RoundTrip(t *testing.T) {
	var b [32]byte
	for i := range b {
		b[i] = byte(i + 0x10)
	}
	original := types.NewInt256(b)
	encoded := bsatn.Encode(original)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadInt256(r)
	require.NoError(t, err)
	assert.Equal(t, original.Bytes(), decoded.Bytes())
}

func TestInt256ExactBytes(t *testing.T) {
	// -1 in two's complement: all 0xFF bytes
	i := types.NewInt256FromU64s(0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF)
	encoded := bsatn.Encode(i)
	expected := make([]byte, 32)
	for idx := range expected {
		expected[idx] = 0xFF
	}
	assert.Equal(t, expected, encoded)
}

func TestInt256Zero(t *testing.T) {
	var zero [32]byte
	i := types.NewInt256(zero)
	assert.True(t, i.IsZero())

	nonZero := types.NewInt256FromU64s(1, 0, 0, 0)
	assert.False(t, nonZero.IsZero())
}

func TestInt256FromU64s(t *testing.T) {
	i := types.NewInt256FromU64s(0xDEADBEEFCAFEBABE, 0x0102030405060708, 0x1111111111111111, 0x8000000000000000)
	b := i.Bytes()
	r := bsatn.NewReader(b[:])

	v0, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(0xDEADBEEFCAFEBABE), v0)

	v1, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(0x0102030405060708), v1)

	v2, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(0x1111111111111111), v2)

	v3, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(0x8000000000000000), v3)
}

func TestInt256String(t *testing.T) {
	i := types.NewInt256FromU64s(1, 0, 0, 0)
	assert.Equal(t, "1", i.String())

	// -1 in two's complement
	neg := types.NewInt256FromU64s(0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF)
	assert.Equal(t, "-1", neg.String())
}

func TestInt256ReadBufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01, 0x02})
	_, err := types.ReadInt256(r)
	require.Error(t, err)
}

// === IDENTITY TESTS ===

func TestIdentityRoundTrip(t *testing.T) {
	var b [32]byte
	for i := range b {
		b[i] = byte(i)
	}
	original := types.NewIdentity(b)
	encoded := bsatn.Encode(original)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadIdentity(r)
	require.NoError(t, err)
	assert.Equal(t, original.Bytes(), decoded.Bytes())
}

func TestIdentityExactBytes(t *testing.T) {
	var b [32]byte
	for i := range b {
		b[i] = byte(i)
	}
	id := types.NewIdentity(b)
	encoded := bsatn.Encode(id)
	// Identity is 32 raw bytes, no length prefix
	assert.Len(t, encoded, 32)
	assert.Equal(t, b[:], encoded)
}

func TestIdentityZero(t *testing.T) {
	var zero [32]byte
	id := types.NewIdentity(zero)
	assert.True(t, id.IsZero())

	var nonZero [32]byte
	nonZero[0] = 1
	id2 := types.NewIdentity(nonZero)
	assert.False(t, id2.IsZero())
}

func TestIdentityString(t *testing.T) {
	var b [32]byte
	// Internal storage is little-endian, so b[0]-b[1] are least significant.
	b[0] = 0xAB
	b[1] = 0xCD
	id := types.NewIdentity(b)
	s := id.String()
	// Display is big-endian hex, so LE bytes [0xAB, 0xCD, 0...] become "0000...00cdab"
	assert.Equal(t,
		"000000000000000000000000000000"+
		"000000000000000000000000000000"+"cdab", s)
}

func TestIdentityFromU64s(t *testing.T) {
	id := types.NewIdentityFromU64s(1, 2, 3, 4)
	b := id.Bytes()

	r := bsatn.NewReader(b[:])
	lo0, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(1), lo0)

	lo1, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(2), lo1)

	hi0, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(3), hi0)

	hi1, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(4), hi1)
}

func TestIdentityReadBufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01, 0x02, 0x03})
	_, err := types.ReadIdentity(r)
	require.Error(t, err)
}

// === CONNECTION ID TESTS ===

func TestConnectionIdRoundTrip(t *testing.T) {
	var b [16]byte
	for i := range b {
		b[i] = byte(i + 0x10)
	}
	original := types.NewConnectionId(b)
	encoded := bsatn.Encode(original)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadConnectionId(r)
	require.NoError(t, err)
	assert.Equal(t, original.Bytes(), decoded.Bytes())
}

func TestConnectionIdExactBytes(t *testing.T) {
	var b [16]byte
	for i := range b {
		b[i] = byte(i)
	}
	cid := types.NewConnectionId(b)
	encoded := bsatn.Encode(cid)
	// ConnectionId is 16 raw bytes
	assert.Len(t, encoded, 16)
	assert.Equal(t, b[:], encoded)
}

func TestConnectionIdZero(t *testing.T) {
	var zero [16]byte
	cid := types.NewConnectionId(zero)
	assert.True(t, cid.IsZero())

	var nonZero [16]byte
	nonZero[0] = 0xFF
	cid2 := types.NewConnectionId(nonZero)
	assert.False(t, cid2.IsZero())
}

func TestConnectionIdFromU64s(t *testing.T) {
	cid := types.NewConnectionIdFromU64s(0xDEADBEEFCAFEBABE, 0x0102030405060708)
	b := cid.Bytes()

	r := bsatn.NewReader(b[:])
	lo, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(0xDEADBEEFCAFEBABE), lo)

	hi, err := r.GetU64()
	require.NoError(t, err)
	assert.Equal(t, uint64(0x0102030405060708), hi)
}

func TestConnectionIdString(t *testing.T) {
	var b [16]byte
	// Internal storage is little-endian, so b[0]-b[1] are least significant.
	b[0] = 0xAB
	b[1] = 0xCD
	cid := types.NewConnectionId(b)
	s := cid.String()
	// Display is big-endian hex, so LE bytes [0xAB, 0xCD, 0...] become "0000...cdab"
	assert.Equal(t,
		"0000000000000000"+
		"000000000000"+"cdab", s)
}

func TestConnectionIdReadBufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01, 0x02})
	_, err := types.ReadConnectionId(r)
	require.Error(t, err)
}

// === TIMESTAMP TESTS ===

func TestTimestampRoundTrip(t *testing.T) {
	original := types.NewTimestamp(1000000) // 1 second in microseconds
	encoded := bsatn.Encode(original)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadTimestamp(r)
	require.NoError(t, err)
	assert.Equal(t, original.Microseconds(), decoded.Microseconds())
}

func TestTimestampExactBytes(t *testing.T) {
	ts := types.NewTimestamp(42)
	encoded := bsatn.Encode(ts)
	// i64 LE encoding of 42
	expected := []byte{0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00}
	assert.Equal(t, expected, encoded)
}

func TestTimestampNegative(t *testing.T) {
	ts := types.NewTimestamp(-1)
	encoded := bsatn.Encode(ts)
	expected := []byte{0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff}
	assert.Equal(t, expected, encoded)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadTimestamp(r)
	require.NoError(t, err)
	assert.Equal(t, int64(-1), decoded.Microseconds())
}

func TestTimestampTime(t *testing.T) {
	micros := int64(1700000000000000) // some time in 2023
	ts := types.NewTimestamp(micros)
	goTime := ts.Time()
	assert.Equal(t, time.UnixMicro(micros), goTime)
}

func TestTimestampString(t *testing.T) {
	ts := types.NewTimestamp(0) // Unix epoch
	s := ts.String()
	// The string representation depends on local timezone, so just check it is non-empty
	assert.NotEmpty(t, s)
	// Also check a known timestamp to verify Time() conversion
	assert.Equal(t, time.UnixMicro(0), ts.Time())
}

func TestTimestampReadBufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01, 0x02, 0x03})
	_, err := types.ReadTimestamp(r)
	require.Error(t, err)
}

// === TIME DURATION TESTS ===

func TestTimeDurationRoundTrip(t *testing.T) {
	original := types.NewTimeDuration(5000000) // 5 seconds
	encoded := bsatn.Encode(original)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadTimeDuration(r)
	require.NoError(t, err)
	assert.Equal(t, original.Microseconds(), decoded.Microseconds())
}

func TestTimeDurationExactBytes(t *testing.T) {
	d := types.NewTimeDuration(1000000) // 1 second in microseconds
	encoded := bsatn.Encode(d)
	// 1000000 = 0x000F4240 LE: {0x40, 0x42, 0x0F, 0x00, 0x00, 0x00, 0x00, 0x00}
	expected := []byte{0x40, 0x42, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x00}
	assert.Equal(t, expected, encoded)
}

func TestTimeDurationDuration(t *testing.T) {
	d := types.NewTimeDuration(1000000) // 1 second
	assert.Equal(t, time.Second, d.Duration())
}

func TestTimeDurationNegative(t *testing.T) {
	d := types.NewTimeDuration(-1000000) // -1 second
	assert.Equal(t, -time.Second, d.Duration())
}

func TestTimeDurationString(t *testing.T) {
	d := types.NewTimeDuration(1500000) // 1.5 seconds
	assert.Equal(t, "1.500000", d.String())

	neg := types.NewTimeDuration(-1500000) // -1.5 seconds
	assert.Equal(t, "-1.500000", neg.String())
}

func TestTimeDurationReadBufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01, 0x02})
	_, err := types.ReadTimeDuration(r)
	require.Error(t, err)
}

// === ENERGY QUANTA TESTS ===

func TestEnergyQuantaRoundTrip(t *testing.T) {
	u128 := types.NewUint128(12345, 0)
	original := types.NewEnergyQuanta(u128)
	encoded := bsatn.Encode(original)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadEnergyQuanta(r)
	require.NoError(t, err)
	assert.Equal(t, original.Value().Bytes(), decoded.Value().Bytes())
}

func TestEnergyQuantaExactBytes(t *testing.T) {
	u128 := types.NewUint128(1, 0)
	eq := types.NewEnergyQuanta(u128)
	encoded := bsatn.Encode(eq)
	// u128 is 16 raw bytes
	expected := []byte{
		0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	}
	assert.Equal(t, expected, encoded)
}

func TestEnergyQuantaString(t *testing.T) {
	u128 := types.NewUint128(42, 0)
	eq := types.NewEnergyQuanta(u128)
	assert.Equal(t, "42", eq.String())
}

func TestEnergyQuantaReadBufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{0x01, 0x02, 0x03})
	_, err := types.ReadEnergyQuanta(r)
	require.Error(t, err)
}

// === SCHEDULE AT TESTS ===

func TestScheduleAtIntervalRoundTrip(t *testing.T) {
	dur := types.NewTimeDuration(5000000) // 5 seconds
	original := types.ScheduleAtInterval{Value: dur}
	encoded := bsatn.Encode(original)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadScheduleAt(r)
	require.NoError(t, err)

	interval, ok := decoded.(types.ScheduleAtInterval)
	require.True(t, ok)
	assert.Equal(t, dur.Microseconds(), interval.Value.Microseconds())
}

func TestScheduleAtTimeRoundTrip(t *testing.T) {
	ts := types.NewTimestamp(1700000000000000)
	original := types.ScheduleAtTime{Value: ts}
	encoded := bsatn.Encode(original)

	r := bsatn.NewReader(encoded)
	decoded, err := types.ReadScheduleAt(r)
	require.NoError(t, err)

	schedTime, ok := decoded.(types.ScheduleAtTime)
	require.True(t, ok)
	assert.Equal(t, ts.Microseconds(), schedTime.Value.Microseconds())
}

func TestScheduleAtIntervalExactBytes(t *testing.T) {
	dur := types.NewTimeDuration(42)
	sa := types.ScheduleAtInterval{Value: dur}
	encoded := bsatn.Encode(sa)
	// tag 0 + i64 LE 42
	expected := []byte{
		0x00,                                              // tag 0 (Interval)
		0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,  // i64 42
	}
	assert.Equal(t, expected, encoded)
}

func TestScheduleAtTimeExactBytes(t *testing.T) {
	ts := types.NewTimestamp(42)
	sa := types.ScheduleAtTime{Value: ts}
	encoded := bsatn.Encode(sa)
	// tag 1 + i64 LE 42
	expected := []byte{
		0x01,                                              // tag 1 (Time)
		0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,  // i64 42
	}
	assert.Equal(t, expected, encoded)
}

func TestScheduleAtInvalidTag(t *testing.T) {
	// tag 5 is invalid
	data := []byte{0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00}
	r := bsatn.NewReader(data)
	_, err := types.ReadScheduleAt(r)
	require.Error(t, err)
	var invalidTag *bsatn.ErrInvalidTag
	assert.ErrorAs(t, err, &invalidTag)
	assert.Equal(t, uint8(5), invalidTag.Tag)
	assert.Equal(t, "ScheduleAt", invalidTag.SumName)
}

func TestScheduleAtReadBufferTooShort(t *testing.T) {
	r := bsatn.NewReader([]byte{})
	_, err := types.ReadScheduleAt(r)
	require.Error(t, err)
}
