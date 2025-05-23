package bsatn

import (
	"bytes"
	"testing"
)

func TestArrays(t *testing.T) {
	// Test U32 arrays
	t.Run("U32Array", func(t *testing.T) {
		testArrays := [][]uint32{
			{},
			{1, 2, 3},
			{0, 4294967295},
			{100, 200, 300, 400, 500},
		}

		for i, array := range testArrays {
			buf := &bytes.Buffer{}
			if err := EncodeU32Array(buf, array); err != nil {
				t.Fatalf("EncodeU32Array(case %d) failed: %v", i, err)
			}

			decoded, err := DecodeU32Array(buf)
			if err != nil {
				t.Fatalf("DecodeU32Array failed: %v", err)
			}

			if len(decoded) != len(array) {
				t.Errorf("Array length mismatch: got %d, want %d", len(decoded), len(array))
				continue
			}

			for j, val := range array {
				if decoded[j] != val {
					t.Errorf("Array element %d mismatch: got %d, want %d", j, decoded[j], val)
				}
			}
		}
	})

	// Test String arrays
	t.Run("StringArray", func(t *testing.T) {
		testArrays := [][]string{
			{},
			{"hello", "world"},
			{"", "test", "🚀"},
			{"one", "two", "three", "four"},
		}

		for i, array := range testArrays {
			buf := &bytes.Buffer{}
			if err := EncodeStringArray(buf, array); err != nil {
				t.Fatalf("EncodeStringArray(case %d) failed: %v", i, err)
			}

			decoded, err := DecodeStringArray(buf)
			if err != nil {
				t.Fatalf("DecodeStringArray failed: %v", err)
			}

			if len(decoded) != len(array) {
				t.Errorf("Array length mismatch: got %d, want %d", len(decoded), len(array))
				continue
			}

			for j, val := range array {
				if decoded[j] != val {
					t.Errorf("Array element %d mismatch: got %q, want %q", j, decoded[j], val)
				}
			}
		}
	})

	// Test F64 arrays
	t.Run("F64Array", func(t *testing.T) {
		testArrays := [][]float64{
			{},
			{1.0, 2.5, 3.14159},
			{0.0, -1.0, 1e10, -1e-10},
		}

		for i, array := range testArrays {
			buf := &bytes.Buffer{}
			if err := EncodeF64Array(buf, array); err != nil {
				t.Fatalf("EncodeF64Array(case %d) failed: %v", i, err)
			}

			decoded, err := DecodeF64Array(buf)
			if err != nil {
				t.Fatalf("DecodeF64Array failed: %v", err)
			}

			if len(decoded) != len(array) {
				t.Errorf("Array length mismatch: got %d, want %d", len(decoded), len(array))
				continue
			}

			for j, val := range array {
				if decoded[j] != val {
					t.Errorf("Array element %d mismatch: got %f, want %f", j, decoded[j], val)
				}
			}
		}
	})
}

func TestOptional(t *testing.T) {
	// Test optional uint32
	t.Run("OptionalU32", func(t *testing.T) {
		testCases := []*uint32{
			nil,
			func() *uint32 { v := uint32(42); return &v }(),
			func() *uint32 { v := uint32(0); return &v }(),
			func() *uint32 { v := uint32(4294967295); return &v }(),
		}

		for i, opt := range testCases {
			buf := &bytes.Buffer{}
			if err := EncodeOptional(buf, opt, EncodeU32); err != nil {
				t.Fatalf("EncodeOptional(case %d) failed: %v", i, err)
			}

			decoded, err := DecodeOptional(buf, DecodeU32)
			if err != nil {
				t.Fatalf("DecodeOptional failed: %v", err)
			}

			if (opt == nil) != (decoded == nil) {
				t.Errorf("Optional nil mismatch: original nil=%v, decoded nil=%v", opt == nil, decoded == nil)
				continue
			}

			if opt != nil && decoded != nil && *opt != *decoded {
				t.Errorf("Optional value mismatch: got %d, want %d", *decoded, *opt)
			}
		}
	})

	// Test optional string
	t.Run("OptionalString", func(t *testing.T) {
		testCases := []*string{
			nil,
			func() *string { v := "hello"; return &v }(),
			func() *string { v := ""; return &v }(),
			func() *string { v := "🚀🎉"; return &v }(),
		}

		for i, opt := range testCases {
			buf := &bytes.Buffer{}
			if err := EncodeOptional(buf, opt, EncodeString); err != nil {
				t.Fatalf("EncodeOptional(case %d) failed: %v", i, err)
			}

			decoded, err := DecodeOptional(buf, DecodeString)
			if err != nil {
				t.Fatalf("DecodeOptional failed: %v", err)
			}

			if (opt == nil) != (decoded == nil) {
				t.Errorf("Optional nil mismatch: original nil=%v, decoded nil=%v", opt == nil, decoded == nil)
				continue
			}

			if opt != nil && decoded != nil && *opt != *decoded {
				t.Errorf("Optional value mismatch: got %q, want %q", *decoded, *opt)
			}
		}
	})
}

func TestMaps(t *testing.T) {
	// Test map[string]uint32
	t.Run("StringToU32Map", func(t *testing.T) {
		testMaps := []map[string]uint32{
			{},
			{"key1": 100, "key2": 200},
			{"": 0, "test": 42, "🚀": 999},
		}

		for i, m := range testMaps {
			buf := &bytes.Buffer{}
			if err := EncodeMap(buf, m, EncodeString, EncodeU32); err != nil {
				t.Fatalf("EncodeMap(case %d) failed: %v", i, err)
			}

			decoded, err := DecodeMap(buf, DecodeString, DecodeU32)
			if err != nil {
				t.Fatalf("DecodeMap failed: %v", err)
			}

			if len(decoded) != len(m) {
				t.Errorf("Map length mismatch: got %d, want %d", len(decoded), len(m))
				continue
			}

			for key, value := range m {
				if decodedValue, exists := decoded[key]; !exists {
					t.Errorf("Missing key %q in decoded map", key)
				} else if decodedValue != value {
					t.Errorf("Map value mismatch for key %q: got %d, want %d", key, decodedValue, value)
				}
			}
		}
	})

	// Test map[uint32]string
	t.Run("U32ToStringMap", func(t *testing.T) {
		testMaps := []map[uint32]string{
			{},
			{1: "one", 2: "two"},
			{0: "", 42: "answer", 999: "🎉"},
		}

		for i, m := range testMaps {
			buf := &bytes.Buffer{}
			if err := EncodeMap(buf, m, EncodeU32, EncodeString); err != nil {
				t.Fatalf("EncodeMap(case %d) failed: %v", i, err)
			}

			decoded, err := DecodeMap(buf, DecodeU32, DecodeString)
			if err != nil {
				t.Fatalf("DecodeMap failed: %v", err)
			}

			if len(decoded) != len(m) {
				t.Errorf("Map length mismatch: got %d, want %d", len(decoded), len(m))
				continue
			}

			for key, value := range m {
				if decodedValue, exists := decoded[key]; !exists {
					t.Errorf("Missing key %d in decoded map", key)
				} else if decodedValue != value {
					t.Errorf("Map value mismatch for key %d: got %q, want %q", key, decodedValue, value)
				}
			}
		}
	})
}

func TestArrayCodecs(t *testing.T) {
	// Test U32ArrayCodec
	t.Run("U32ArrayCodec", func(t *testing.T) {
		original := []uint32{1, 2, 3, 4, 5}
		codec := &U32ArrayCodec{Value: original}

		// Test encoding
		buf := &bytes.Buffer{}
		if err := codec.Encode(buf); err != nil {
			t.Fatalf("Encode failed: %v", err)
		}

		// Test size calculation
		expectedSize := SizeU32Array(original)
		actualSize := codec.BsatnSize()
		if actualSize != expectedSize {
			t.Errorf("Size mismatch: got %d, want %d", actualSize, expectedSize)
		}

		// Test decoding
		decodedCodec := &U32ArrayCodec{}
		if err := decodedCodec.Decode(buf); err != nil {
			t.Fatalf("Decode failed: %v", err)
		}

		if len(decodedCodec.Value) != len(original) {
			t.Errorf("Array length mismatch: got %d, want %d", len(decodedCodec.Value), len(original))
		}

		for i, val := range original {
			if decodedCodec.Value[i] != val {
				t.Errorf("Array element %d mismatch: got %d, want %d", i, decodedCodec.Value[i], val)
			}
		}
	})

	// Test StringArrayCodec
	t.Run("StringArrayCodec", func(t *testing.T) {
		original := []string{"hello", "world", "test", "🚀"}
		codec := &StringArrayCodec{Value: original}

		// Test encoding
		buf := &bytes.Buffer{}
		if err := codec.Encode(buf); err != nil {
			t.Fatalf("Encode failed: %v", err)
		}

		// Test size calculation
		expectedSize := SizeStringArray(original)
		actualSize := codec.BsatnSize()
		if actualSize != expectedSize {
			t.Errorf("Size mismatch: got %d, want %d", actualSize, expectedSize)
		}

		// Test decoding
		decodedCodec := &StringArrayCodec{}
		if err := decodedCodec.Decode(buf); err != nil {
			t.Fatalf("Decode failed: %v", err)
		}

		if len(decodedCodec.Value) != len(original) {
			t.Errorf("Array length mismatch: got %d, want %d", len(decodedCodec.Value), len(original))
		}

		for i, val := range original {
			if decodedCodec.Value[i] != val {
				t.Errorf("Array element %d mismatch: got %q, want %q", i, decodedCodec.Value[i], val)
			}
		}
	})
}

func TestCollectionUtilityFunctions(t *testing.T) {
	// Test U32ArrayToBytes and U32ArrayFromBytes
	t.Run("U32ArrayBytes", func(t *testing.T) {
		original := []uint32{10, 20, 30, 40, 50}

		data, err := U32ArrayToBytes(original)
		if err != nil {
			t.Fatalf("U32ArrayToBytes failed: %v", err)
		}

		decoded, err := U32ArrayFromBytes(data)
		if err != nil {
			t.Fatalf("U32ArrayFromBytes failed: %v", err)
		}

		if len(decoded) != len(original) {
			t.Errorf("Array length mismatch: got %d, want %d", len(decoded), len(original))
		}

		for i, val := range original {
			if decoded[i] != val {
				t.Errorf("Array element %d mismatch: got %d, want %d", i, decoded[i], val)
			}
		}
	})

	// Test StringArrayToBytes and StringArrayFromBytes
	t.Run("StringArrayBytes", func(t *testing.T) {
		original := []string{"alpha", "beta", "gamma", "delta"}

		data, err := StringArrayToBytes(original)
		if err != nil {
			t.Fatalf("StringArrayToBytes failed: %v", err)
		}

		decoded, err := StringArrayFromBytes(data)
		if err != nil {
			t.Fatalf("StringArrayFromBytes failed: %v", err)
		}

		if len(decoded) != len(original) {
			t.Errorf("Array length mismatch: got %d, want %d", len(decoded), len(original))
		}

		for i, val := range original {
			if decoded[i] != val {
				t.Errorf("Array element %d mismatch: got %q, want %q", i, decoded[i], val)
			}
		}
	})
}

func TestStructEncoding(t *testing.T) {
	// Test struct encoding with reflection
	t.Run("SimpleStruct", func(t *testing.T) {
		type TestStruct struct {
			ID     uint32
			Name   string
			Score  float64
			Active bool
		}

		original := TestStruct{
			ID:     42,
			Name:   "test",
			Score:  95.5,
			Active: true,
		}

		buf := &bytes.Buffer{}
		if err := EncodeStruct(buf, original); err != nil {
			t.Fatalf("EncodeStruct failed: %v", err)
		}

		// For now, just verify that encoding doesn't crash
		// Full round-trip struct decoding would require more complex reflection logic
		if buf.Len() == 0 {
			t.Error("Expected non-empty encoded data")
		}
	})
}

func TestCollectionErrorHandling(t *testing.T) {
	// Test array too long
	t.Run("ArrayTooLong", func(t *testing.T) {
		buf := &bytes.Buffer{}
		// Encode a length that's too large
		EncodeU32(buf, 1<<21) // 2M elements, exceeds 1M limit
		_, err := DecodeU32Array(buf)
		if err == nil {
			t.Error("Expected error for array too long")
		}
	})

	// Test map too long
	t.Run("MapTooLong", func(t *testing.T) {
		buf := &bytes.Buffer{}
		// Encode a length that's too large
		EncodeU32(buf, 1<<21) // 2M entries, exceeds 1M limit
		_, err := DecodeMap(buf, DecodeString, DecodeU32)
		if err == nil {
			t.Error("Expected error for map too long")
		}
	})

	// Test invalid optional tag
	t.Run("InvalidOptionalTag", func(t *testing.T) {
		buf := bytes.NewBuffer([]byte{2}) // Invalid tag
		_, err := DecodeOptional(buf, DecodeU32)
		if err == nil {
			t.Error("Expected error for invalid optional tag")
		}
	})
}

// Benchmark tests for collections
func BenchmarkEncodeU32Array(b *testing.B) {
	array := make([]uint32, 1000)
	for i := range array {
		array[i] = uint32(i)
	}

	buf := &bytes.Buffer{}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		buf.Reset()
		EncodeU32Array(buf, array)
	}
}

func BenchmarkDecodeU32Array(b *testing.B) {
	array := make([]uint32, 1000)
	for i := range array {
		array[i] = uint32(i)
	}

	// Pre-encode data
	buf := &bytes.Buffer{}
	EncodeU32Array(buf, array)
	data := buf.Bytes()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		reader := bytes.NewReader(data)
		DecodeU32Array(reader)
	}
}

func BenchmarkEncodeStringArray(b *testing.B) {
	array := make([]string, 100)
	for i := range array {
		array[i] = "test_string_" + string(rune('0'+i%10))
	}

	buf := &bytes.Buffer{}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		buf.Reset()
		EncodeStringArray(buf, array)
	}
}

func BenchmarkDecodeStringArray(b *testing.B) {
	array := make([]string, 100)
	for i := range array {
		array[i] = "test_string_" + string(rune('0'+i%10))
	}

	// Pre-encode data
	buf := &bytes.Buffer{}
	EncodeStringArray(buf, array)
	data := buf.Bytes()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		reader := bytes.NewReader(data)
		DecodeStringArray(reader)
	}
}
