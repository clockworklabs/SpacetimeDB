package bsatn

import (
	"errors"
	"math"
	"reflect"
	"testing"
)

func TestUnmarshalIntoOverflow(t *testing.T) {
	type targetUint8 struct {
		Val uint8 `bsatn:"val"`
	}
	type targetInt8 struct {
		Val int8 `bsatn:"val"`
	}
	type targetUint16 struct {
		Val uint16 `bsatn:"val"`
	}
	type targetInt16 struct {
		Val int16 `bsatn:"val"`
	}
	type targetUint32 struct {
		Val uint32 `bsatn:"val"`
	}
	type targetInt32 struct {
		Val int32 `bsatn:"val"`
	}
	type targetUint64 struct {
		Val uint64 `bsatn:"val"`
	}
	type targetInt64 struct {
		Val int64 `bsatn:"val"`
	}

	testCases := []struct {
		name        string
		source      interface{}
		targetPtr   interface{}
		expectError error
		expectValue interface{}
	}{
		// Unsigned to Unsigned Overflow
		{
			name:        "uint16_to_uint8_overflow",
			source:      map[string]interface{}{"val": uint16(256)},
			targetPtr:   &targetUint8{},
			expectError: ErrOverflow,
		},
		{
			name:        "uint32_to_uint16_overflow",
			source:      map[string]interface{}{"val": uint32(65536)},
			targetPtr:   &targetUint16{},
			expectError: ErrOverflow,
		},
		{
			name:        "uint64_to_uint32_overflow",
			source:      map[string]interface{}{"val": uint64(math.MaxUint32 + 1)},
			targetPtr:   &targetUint32{},
			expectError: ErrOverflow,
		},
		// Signed to Signed Overflow/Underflow
		{
			name:        "int16_to_int8_overflow",
			source:      map[string]interface{}{"val": int16(128)},
			targetPtr:   &targetInt8{},
			expectError: ErrOverflow,
		},
		{
			name:        "int16_to_int8_underflow",
			source:      map[string]interface{}{"val": int16(-129)},
			targetPtr:   &targetInt8{},
			expectError: ErrOverflow,
		},
		{
			name:        "int32_to_int16_overflow",
			source:      map[string]interface{}{"val": int32(math.MaxInt16 + 1)},
			targetPtr:   &targetInt16{},
			expectError: ErrOverflow,
		},
		{
			name:        "int32_to_int16_underflow",
			source:      map[string]interface{}{"val": int32(math.MinInt16 - 1)},
			targetPtr:   &targetInt16{},
			expectError: ErrOverflow,
		},
		// Unsigned to Signed Overflow
		{
			name:        "uint8_to_int8_overflow",
			source:      map[string]interface{}{"val": uint8(128)},
			targetPtr:   &targetInt8{},
			expectError: ErrOverflow,
		},
		{
			name:        "uint64_to_int64_overflow",
			source:      map[string]interface{}{"val": uint64(math.MaxInt64 + 1)},
			targetPtr:   &targetInt64{},
			expectError: ErrOverflow,
		},
		// Negative to Unsigned (should error)
		{
			name:        "int8_to_uint8_negative",
			source:      map[string]interface{}{"val": int8(-1)},
			targetPtr:   &targetUint8{},
			expectError: ErrOverflow, // Or a more specific conversion error from fmt.Errorf
		},
		// Valid conversions
		{
			name:        "uint8_to_uint16_valid",
			source:      map[string]interface{}{"val": uint8(100)},
			targetPtr:   &targetUint16{},
			expectValue: &targetUint16{Val: 100},
		},
		{
			name:        "int8_to_int16_valid",
			source:      map[string]interface{}{"val": int8(-10)},
			targetPtr:   &targetInt16{},
			expectValue: &targetInt16{Val: -10},
		},
		{
			name:        "uint16_to_uint8_valid",
			source:      map[string]interface{}{"val": uint16(200)},
			targetPtr:   &targetUint8{},
			expectValue: &targetUint8{Val: 200},
		},
		{
			name:        "int16_to_int8_valid",
			source:      map[string]interface{}{"val": int16(120)},
			targetPtr:   &targetInt8{},
			expectValue: &targetInt8{Val: 120},
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			// Marshal the source map to BSATN
			bsatnBytes, err := Marshal(tc.source)
			if err != nil {
				t.Fatalf("Marshal failed: %v", err)
			}

			err = UnmarshalInto(bsatnBytes, tc.targetPtr)

			if tc.expectError != nil {
				if !errors.Is(err, tc.expectError) {
					t.Errorf("Expected error %v, got %v", tc.expectError, err)
				}
			} else {
				if err != nil {
					t.Errorf("Expected no error, got %v", err)
				}
				if !reflect.DeepEqual(tc.targetPtr, tc.expectValue) {
					t.Errorf("Value mismatch: got %#v, want %#v", tc.targetPtr, tc.expectValue)
				}
			}
		})
	}
}
