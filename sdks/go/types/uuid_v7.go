package types

import "fmt"

// NewUuidV7 generates a UUID v7 using a monotonic counter, a timestamp in microseconds
// since the Unix epoch, and 4 random bytes.
//
// The counter wraps at 0x7FFFFFFF (31-bit max). The counter is incremented after each call.
//
// The UUID v7 layout (big-endian byte order):
//
//	bytes[0..5]:  unix_ts_ms (48 bits)
//	bytes[6]:     version (0x7x) | counter bits
//	bytes[7]:     counter_high
//	bytes[8]:     variant (0x8x) | counter bits
//	bytes[9..11]: counter_low
//	bytes[12..15]: random
//
// The result is stored in BSATN u128 little-endian format (reversed from big-endian UUID).
func NewUuidV7(counter *uint32, timestampMicros int64, randomBytes [4]byte) (Uuid, error) {
	if timestampMicros < 0 {
		return nil, fmt.Errorf("timestamp before unix epoch")
	}

	// Get counter value and increment (wrapping at 31-bit max).
	counterVal := *counter
	*counter = (counterVal + 1) & 0x7FFFFFFF

	// Convert timestamp from microseconds to milliseconds, masked to 48 bits.
	tsMs := (timestampMicros / 1000) & 0xFFFFFFFFFFFF

	// Build UUID bytes in RFC 4122 big-endian order.
	var bytes [16]byte

	// unix_ts_ms (48 bits, big-endian)
	bytes[0] = byte(tsMs >> 40)
	bytes[1] = byte(tsMs >> 32)
	bytes[2] = byte(tsMs >> 24)
	bytes[3] = byte(tsMs >> 16)
	bytes[4] = byte(tsMs >> 8)
	bytes[5] = byte(tsMs)

	// Counter bits (matching Rust layout exactly)
	bytes[7] = byte((counterVal >> 23) & 0xFF)
	bytes[9] = byte((counterVal >> 15) & 0xFF)
	bytes[10] = byte((counterVal >> 7) & 0xFF)
	bytes[11] = byte((counterVal & 0x7F) << 1)

	// Random bytes
	bytes[12] |= randomBytes[0] & 0x7F
	bytes[13] = randomBytes[1]
	bytes[14] = randomBytes[2]
	bytes[15] = randomBytes[3]

	// Apply version 7: high nibble of byte 6 = 0x70
	bytes[6] = (bytes[6] & 0x0F) | 0x70

	// Apply RFC 4122 variant: top 2 bits of byte 8 = 10
	bytes[8] = (bytes[8] & 0x3F) | 0x80

	// Convert big-endian UUID bytes to little-endian u128 for BSATN storage.
	// Reverse the 16 bytes.
	var le [16]byte
	for i := 0; i < 16; i++ {
		le[i] = bytes[15-i]
	}

	return &uuidImpl{data: le}, nil
}
