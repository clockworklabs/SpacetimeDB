package bsatn

import (
	"bytes"
	"io"
	"math"
	"testing"
)

func TestPrimitiveTypes(t *testing.T) {
	// Test all primitive type encoders/decoders

	// Test U8
	t.Run("U8", func(t *testing.T) {
		testValues := []uint8{0, 1, 127, 255}
		for _, val := range testValues {
			buf := &bytes.Buffer{}
			if err := EncodeU8(buf, val); err != nil {
				t.Fatalf("EncodeU8(%d) failed: %v", val, err)
			}
			decoded, err := DecodeU8(buf)
			if err != nil {
				t.Fatalf("DecodeU8 failed: %v", err)
			}
			if decoded != val {
				t.Errorf("U8 roundtrip failed: got %d, want %d", decoded, val)
			}
		}
	})

	// Test U16
	t.Run("U16", func(t *testing.T) {
		testValues := []uint16{0, 1, 256, 32767, 65535}
		for _, val := range testValues {
			buf := &bytes.Buffer{}
			if err := EncodeU16(buf, val); err != nil {
				t.Fatalf("EncodeU16(%d) failed: %v", val, err)
			}
			decoded, err := DecodeU16(buf)
			if err != nil {
				t.Fatalf("DecodeU16 failed: %v", err)
			}
			if decoded != val {
				t.Errorf("U16 roundtrip failed: got %d, want %d", decoded, val)
			}
		}
	})

	// Test U32
	t.Run("U32", func(t *testing.T) {
		testValues := []uint32{0, 1, 65536, 2147483647, 4294967295}
		for _, val := range testValues {
			buf := &bytes.Buffer{}
			if err := EncodeU32(buf, val); err != nil {
				t.Fatalf("EncodeU32(%d) failed: %v", val, err)
			}
			decoded, err := DecodeU32(buf)
			if err != nil {
				t.Fatalf("DecodeU32 failed: %v", err)
			}
			if decoded != val {
				t.Errorf("U32 roundtrip failed: got %d, want %d", decoded, val)
			}
		}
	})

	// Test U64
	t.Run("U64", func(t *testing.T) {
		testValues := []uint64{0, 1, 4294967296, 9223372036854775807, 18446744073709551615}
		for _, val := range testValues {
			buf := &bytes.Buffer{}
			if err := EncodeU64(buf, val); err != nil {
				t.Fatalf("EncodeU64(%d) failed: %v", val, err)
			}
			decoded, err := DecodeU64(buf)
			if err != nil {
				t.Fatalf("DecodeU64 failed: %v", err)
			}
			if decoded != val {
				t.Errorf("U64 roundtrip failed: got %d, want %d", decoded, val)
			}
		}
	})

	// Test I8
	t.Run("I8", func(t *testing.T) {
		testValues := []int8{-128, -1, 0, 1, 127}
		for _, val := range testValues {
			buf := &bytes.Buffer{}
			if err := EncodeI8(buf, val); err != nil {
				t.Fatalf("EncodeI8(%d) failed: %v", val, err)
			}
			decoded, err := DecodeI8(buf)
			if err != nil {
				t.Fatalf("DecodeI8 failed: %v", err)
			}
			if decoded != val {
				t.Errorf("I8 roundtrip failed: got %d, want %d", decoded, val)
			}
		}
	})

	// Test I16
	t.Run("I16", func(t *testing.T) {
		testValues := []int16{-32768, -1, 0, 1, 32767}
		for _, val := range testValues {
			buf := &bytes.Buffer{}
			if err := EncodeI16(buf, val); err != nil {
				t.Fatalf("EncodeI16(%d) failed: %v", val, err)
			}
			decoded, err := DecodeI16(buf)
			if err != nil {
				t.Fatalf("DecodeI16 failed: %v", err)
			}
			if decoded != val {
				t.Errorf("I16 roundtrip failed: got %d, want %d", decoded, val)
			}
		}
	})

	// Test I32
	t.Run("I32", func(t *testing.T) {
		testValues := []int32{-2147483648, -1, 0, 1, 2147483647}
		for _, val := range testValues {
			buf := &bytes.Buffer{}
			if err := EncodeI32(buf, val); err != nil {
				t.Fatalf("EncodeI32(%d) failed: %v", val, err)
			}
			decoded, err := DecodeI32(buf)
			if err != nil {
				t.Fatalf("DecodeI32 failed: %v", err)
			}
			if decoded != val {
				t.Errorf("I32 roundtrip failed: got %d, want %d", decoded, val)
			}
		}
	})

	// Test I64
	t.Run("I64", func(t *testing.T) {
		testValues := []int64{-9223372036854775808, -1, 0, 1, 9223372036854775807}
		for _, val := range testValues {
			buf := &bytes.Buffer{}
			if err := EncodeI64(buf, val); err != nil {
				t.Fatalf("EncodeI64(%d) failed: %v", val, err)
			}
			decoded, err := DecodeI64(buf)
			if err != nil {
				t.Fatalf("DecodeI64 failed: %v", err)
			}
			if decoded != val {
				t.Errorf("I64 roundtrip failed: got %d, want %d", decoded, val)
			}
		}
	})

	// Test F32
	t.Run("F32", func(t *testing.T) {
		testValues := []float32{
			0.0,
			-0.0,
			1.0,
			-1.0,
			math.MaxFloat32,
			-math.MaxFloat32,
			math.SmallestNonzeroFloat32,
			float32(math.Inf(1)),
			float32(math.Inf(-1)),
			float32(math.NaN()),
		}
		for _, val := range testValues {
			buf := &bytes.Buffer{}
			if err := EncodeF32(buf, val); err != nil {
				t.Fatalf("EncodeF32(%f) failed: %v", val, err)
			}
			decoded, err := DecodeF32(buf)
			if err != nil {
				t.Fatalf("DecodeF32 failed: %v", err)
			}
			// Special handling for NaN
			if math.IsNaN(float64(val)) {
				if !math.IsNaN(float64(decoded)) {
					t.Errorf("F32 NaN roundtrip failed: got %f, want NaN", decoded)
				}
			} else if decoded != val {
				t.Errorf("F32 roundtrip failed: got %f, want %f", decoded, val)
			}
		}
	})

	// Test F64
	t.Run("F64", func(t *testing.T) {
		testValues := []float64{
			0.0,
			-0.0,
			1.0,
			-1.0,
			math.MaxFloat64,
			-math.MaxFloat64,
			math.SmallestNonzeroFloat64,
			math.Inf(1),
			math.Inf(-1),
			math.NaN(),
		}
		for _, val := range testValues {
			buf := &bytes.Buffer{}
			if err := EncodeF64(buf, val); err != nil {
				t.Fatalf("EncodeF64(%f) failed: %v", val, err)
			}
			decoded, err := DecodeF64(buf)
			if err != nil {
				t.Fatalf("DecodeF64 failed: %v", err)
			}
			// Special handling for NaN
			if math.IsNaN(val) {
				if !math.IsNaN(decoded) {
					t.Errorf("F64 NaN roundtrip failed: got %f, want NaN", decoded)
				}
			} else if decoded != val {
				t.Errorf("F64 roundtrip failed: got %f, want %f", decoded, val)
			}
		}
	})

	// Test Bool
	t.Run("Bool", func(t *testing.T) {
		testValues := []bool{true, false}
		for _, val := range testValues {
			buf := &bytes.Buffer{}
			if err := EncodeBool(buf, val); err != nil {
				t.Fatalf("EncodeBool(%t) failed: %v", val, err)
			}
			decoded, err := DecodeBool(buf)
			if err != nil {
				t.Fatalf("DecodeBool failed: %v", err)
			}
			if decoded != val {
				t.Errorf("Bool roundtrip failed: got %t, want %t", decoded, val)
			}
		}
	})

	// Test String
	t.Run("String", func(t *testing.T) {
		testValues := []string{
			"",
			"hello",
			"Hello, 世界",
			"🚀🎉✨",                      // Emojis
			string(make([]byte, 1000)), // Large string
		}
		for _, val := range testValues {
			buf := &bytes.Buffer{}
			if err := EncodeString(buf, val); err != nil {
				t.Fatalf("EncodeString(%q) failed: %v", val, err)
			}
			decoded, err := DecodeString(buf)
			if err != nil {
				t.Fatalf("DecodeString failed: %v", err)
			}
			if decoded != val {
				t.Errorf("String roundtrip failed: got %q, want %q", decoded, val)
			}
		}
	})

	// Test Bytes
	t.Run("Bytes", func(t *testing.T) {
		testValues := [][]byte{
			{},
			{0},
			{1, 2, 3, 4, 5},
			{255, 254, 253},
			make([]byte, 1000), // Large byte array
		}
		for i, val := range testValues {
			buf := &bytes.Buffer{}
			if err := EncodeBytes(buf, val); err != nil {
				t.Fatalf("EncodeBytes(case %d) failed: %v", i, err)
			}
			decoded, err := DecodeBytes(buf)
			if err != nil {
				t.Fatalf("DecodeBytes failed: %v", err)
			}
			if !bytes.Equal(decoded, val) {
				t.Errorf("Bytes roundtrip failed: got %v, want %v", decoded, val)
			}
		}
	})
}

func TestErrorHandling(t *testing.T) {
	// Test invalid Bool value
	t.Run("InvalidBool", func(t *testing.T) {
		buf := bytes.NewBuffer([]byte{2}) // Invalid bool value
		_, err := DecodeBool(buf)
		if err == nil {
			t.Error("Expected error for invalid bool value")
		}
		if decodingErr, ok := err.(*DecodingError); ok {
			if decodingErr.Type != "bool" {
				t.Errorf("Expected bool error, got %s", decodingErr.Type)
			}
		} else {
			t.Errorf("Expected DecodingError, got %T", err)
		}
	})

	// Test string too long
	t.Run("StringTooLong", func(t *testing.T) {
		buf := &bytes.Buffer{}
		// Encode a length that's too large
		EncodeU32(buf, 1<<25) // 32MB, exceeds 16MB limit
		_, err := DecodeString(buf)
		if err == nil {
			t.Error("Expected error for string too long")
		}
	})

	// Test bytes too long
	t.Run("BytesTooLong", func(t *testing.T) {
		buf := &bytes.Buffer{}
		// Encode a length that's too large
		EncodeU32(buf, 1<<27) // 128MB, exceeds 64MB limit
		_, err := DecodeBytes(buf)
		if err == nil {
			t.Error("Expected error for bytes too long")
		}
	})

	// Test EOF errors
	t.Run("EOFErrors", func(t *testing.T) {
		emptyBuf := &bytes.Buffer{}

		_, err := DecodeU8(emptyBuf)
		if err == nil {
			t.Error("Expected EOF error for U8")
		}

		_, err = DecodeU32(emptyBuf)
		if err == nil {
			t.Error("Expected EOF error for U32")
		}

		_, err = DecodeString(emptyBuf)
		if err == nil {
			t.Error("Expected EOF error for String")
		}
	})
}

func TestSizeFunctions(t *testing.T) {
	// Test all size calculation functions
	tests := []struct {
		name     string
		sizeFunc func() int
		expected int
	}{
		{"SizeU8", SizeU8, 1},
		{"SizeU16", SizeU16, 2},
		{"SizeU32", SizeU32, 4},
		{"SizeU64", SizeU64, 8},
		{"SizeI8", SizeI8, 1},
		{"SizeI16", SizeI16, 2},
		{"SizeI32", SizeI32, 4},
		{"SizeI64", SizeI64, 8},
		{"SizeF32", SizeF32, 4},
		{"SizeF64", SizeF64, 8},
		{"SizeBool", SizeBool, 1},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.sizeFunc(); got != tt.expected {
				t.Errorf("%s() = %d, want %d", tt.name, got, tt.expected)
			}
		})
	}

	// Test variable-size functions
	t.Run("SizeString", func(t *testing.T) {
		tests := []struct {
			input    string
			expected int
		}{
			{"", 4},           // Just the length prefix
			{"hello", 9},      // 4 (length) + 5 (data)
			{"Hello, 世界", 17}, // 4 (length) + 13 (UTF-8 bytes)
		}
		for _, tt := range tests {
			if got := SizeString(tt.input); got != tt.expected {
				t.Errorf("SizeString(%q) = %d, want %d", tt.input, got, tt.expected)
			}
		}
	})

	t.Run("SizeBytes", func(t *testing.T) {
		tests := []struct {
			input    []byte
			expected int
		}{
			{[]byte{}, 4},            // Just the length prefix
			{[]byte{1, 2, 3}, 7},     // 4 (length) + 3 (data)
			{make([]byte, 100), 104}, // 4 (length) + 100 (data)
		}
		for _, tt := range tests {
			if got := SizeBytes(tt.input); got != tt.expected {
				t.Errorf("SizeBytes(len=%d) = %d, want %d", len(tt.input), got, tt.expected)
			}
		}
	})
}

func TestUtilityFunctions(t *testing.T) {
	// Test ToBytes and FromBytes utility functions
	t.Run("ToBytes", func(t *testing.T) {
		data, err := ToBytes(func(w io.Writer) error {
			return EncodeU32(w, 12345)
		})
		if err != nil {
			t.Fatalf("ToBytes failed: %v", err)
		}
		if len(data) != 4 {
			t.Errorf("Expected 4 bytes, got %d", len(data))
		}

		// Verify the data is correct
		buf := bytes.NewBuffer(data)
		decoded, err := DecodeU32(buf)
		if err != nil {
			t.Fatalf("Decode failed: %v", err)
		}
		if decoded != 12345 {
			t.Errorf("Expected 12345, got %d", decoded)
		}
	})

	t.Run("FromBytes", func(t *testing.T) {
		// Encode some data first
		original := uint32(67890)
		buf := &bytes.Buffer{}
		if err := EncodeU32(buf, original); err != nil {
			t.Fatalf("Encode failed: %v", err)
		}
		data := buf.Bytes()

		// Now use FromBytes to decode
		var decoded uint32
		err := FromBytes(data, func(r io.Reader) error {
			var err error
			decoded, err = DecodeU32(r)
			return err
		})
		if err != nil {
			t.Fatalf("FromBytes failed: %v", err)
		}
		if decoded != original {
			t.Errorf("Expected %d, got %d", original, decoded)
		}
	})
}

func TestTypeConstants(t *testing.T) {
	// Test that type tag constants are defined correctly
	expectedTags := map[string]int{
		"TypeTagU8":     0,
		"TypeTagU16":    1,
		"TypeTagU32":    2,
		"TypeTagU64":    3,
		"TypeTagU128":   4,
		"TypeTagU256":   5,
		"TypeTagI8":     6,
		"TypeTagI16":    7,
		"TypeTagI32":    8,
		"TypeTagI64":    9,
		"TypeTagI128":   10,
		"TypeTagI256":   11,
		"TypeTagF32":    12,
		"TypeTagF64":    13,
		"TypeTagBool":   14,
		"TypeTagString": 15,
		"TypeTagBytes":  16,
	}

	actualTags := map[string]int{
		"TypeTagU8":     TypeTagU8,
		"TypeTagU16":    TypeTagU16,
		"TypeTagU32":    TypeTagU32,
		"TypeTagU64":    TypeTagU64,
		"TypeTagU128":   TypeTagU128,
		"TypeTagU256":   TypeTagU256,
		"TypeTagI8":     TypeTagI8,
		"TypeTagI16":    TypeTagI16,
		"TypeTagI32":    TypeTagI32,
		"TypeTagI64":    TypeTagI64,
		"TypeTagI128":   TypeTagI128,
		"TypeTagI256":   TypeTagI256,
		"TypeTagF32":    TypeTagF32,
		"TypeTagF64":    TypeTagF64,
		"TypeTagBool":   TypeTagBool,
		"TypeTagString": TypeTagString,
		"TypeTagBytes":  TypeTagBytes,
	}

	for name, expected := range expectedTags {
		if actual := actualTags[name]; actual != expected {
			t.Errorf("%s = %d, want %d", name, actual, expected)
		}
	}
}

// Benchmark tests
func BenchmarkEncodeU32(b *testing.B) {
	buf := &bytes.Buffer{}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		buf.Reset()
		EncodeU32(buf, uint32(i))
	}
}

func BenchmarkDecodeU32(b *testing.B) {
	// Pre-encode data
	buf := &bytes.Buffer{}
	EncodeU32(buf, 12345)
	data := buf.Bytes()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		reader := bytes.NewReader(data)
		DecodeU32(reader)
	}
}

func BenchmarkEncodeString(b *testing.B) {
	testString := "Hello, SpacetimeDB! This is a test string for benchmarking."
	buf := &bytes.Buffer{}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		buf.Reset()
		EncodeString(buf, testString)
	}
}

func BenchmarkDecodeString(b *testing.B) {
	// Pre-encode data
	testString := "Hello, SpacetimeDB! This is a test string for benchmarking."
	buf := &bytes.Buffer{}
	EncodeString(buf, testString)
	data := buf.Bytes()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		reader := bytes.NewReader(data)
		DecodeString(reader)
	}
}

func BenchmarkEncodeF64(b *testing.B) {
	buf := &bytes.Buffer{}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		buf.Reset()
		EncodeF64(buf, math.Pi)
	}
}

func BenchmarkDecodeF64(b *testing.B) {
	// Pre-encode data
	buf := &bytes.Buffer{}
	EncodeF64(buf, math.Pi)
	data := buf.Bytes()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		reader := bytes.NewReader(data)
		DecodeF64(reader)
	}
}
