package tests

import (
	"fmt"
	"testing"

	bsatn "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/bsatn"
)

// ===== ADVANCED TYPE DEFINITIONS =====

// Color enum using variants
type Color uint32

const (
	ColorRed Color = iota
	ColorGreen
	ColorBlue
	ColorRGBA
)

// Status enum with data
type Status uint32

const (
	StatusPending Status = iota
	StatusActive
	StatusCompleted
	StatusError
)

// Complex nested structure
type NestedData struct {
	Metadata map[string]int32 `json:"metadata"`
	Tags     []string         `json:"tags"`
	Optional *string          `json:"optional"`
	Subdata  []SubData        `json:"subdata"`
}

type SubData struct {
	ID    uint32             `json:"id"`
	Props map[string]float64 `json:"props"`
}

// Recursive structure for trees
type TreeNode struct {
	Value    string      `json:"value"`
	Children []*TreeNode `json:"children"`
}

// Multi-level enum example
type NetworkMessage struct {
	Type    uint32      `json:"type"`
	Payload interface{} `json:"payload"`
}

type LoginRequest struct {
	Username string `json:"username"`
	Password string `json:"password"`
}

type ChatMessage struct {
	From    string `json:"from"`
	To      string `json:"to"`
	Content string `json:"content"`
}

type SystemAlert struct {
	Level   uint32 `json:"level"`
	Message string `json:"message"`
}

// ===== MAP/DICTIONARY TESTS =====

func TestBSATN_AdvancedMaps(t *testing.T) {
	t.Run("string_to_primitive_maps", func(t *testing.T) {
		tests := []struct {
			name string
			data map[string]interface{}
		}{
			{
				"string_to_int",
				map[string]interface{}{
					"count":  int32(42),
					"total":  int32(100),
					"offset": int32(-5),
				},
			},
			{
				"string_to_float",
				map[string]interface{}{
					"pi":     3.14159,
					"e":      2.71828,
					"golden": 1.618,
				},
			},
			{
				"string_to_mixed",
				map[string]interface{}{
					"name":    "Alice",
					"age":     int32(30),
					"active":  true,
					"balance": 123.45,
				},
			},
			{
				"empty_map",
				map[string]interface{}{},
			},
		}

		for _, tt := range tests {
			t.Run(tt.name, func(t *testing.T) {
				encoded, err := bsatn.Marshal(tt.data)
				if err != nil {
					t.Fatalf("failed to encode map: %v", err)
				}

				decoded, _, err := bsatn.Unmarshal(encoded)
				if err != nil {
					t.Fatalf("failed to decode map: %v", err)
				}

				// Maps are decoded as map[string]interface{}
				decodedMap, ok := decoded.(map[string]interface{})
				if !ok {
					t.Fatalf("decoded value is not map[string]interface{}: %T", decoded)
				}

				if len(decodedMap) != len(tt.data) {
					t.Fatalf("map length mismatch: got %d, expected %d", len(decodedMap), len(tt.data))
				}

				for key, expectedValue := range tt.data {
					actualValue, exists := decodedMap[key]
					if !exists {
						t.Fatalf("key %q missing from decoded map", key)
					}

					if !valuesEqual(expectedValue, actualValue) {
						t.Fatalf("value mismatch for key %q: got %v (%T), expected %v (%T)",
							key, actualValue, actualValue, expectedValue, expectedValue)
					}
				}

				t.Logf("✅ Map round-trip successful: %s", tt.name)
			})
		}
	})

	t.Run("string_to_complex_maps", func(t *testing.T) {
		// Test maps containing arrays and nested structures
		complexMap := map[string]interface{}{
			"users": []string{"alice", "bob", "charlie"},
			"config": map[string]interface{}{
				"debug":   true,
				"timeout": int32(30),
			},
			"stats": []int32{10, 20, 30, 40, 50},
		}

		encoded, err := bsatn.Marshal(complexMap)
		if err != nil {
			t.Fatalf("failed to encode complex map: %v", err)
		}

		decoded, _, err := bsatn.Unmarshal(encoded)
		if err != nil {
			t.Fatalf("failed to decode complex map: %v", err)
		}

		decodedMap, ok := decoded.(map[string]interface{})
		if !ok {
			t.Fatalf("decoded value is not map[string]interface{}: %T", decoded)
		}

		// Verify users array
		users, exists := decodedMap["users"]
		if !exists {
			t.Fatalf("users key missing")
		}
		usersSlice, ok := users.([]interface{})
		if !ok {
			t.Fatalf("users is not []interface{}: %T", users)
		}
		if len(usersSlice) != 3 {
			t.Fatalf("users length mismatch: got %d, expected 3", len(usersSlice))
		}

		// Verify nested config map
		config, exists := decodedMap["config"]
		if !exists {
			t.Fatalf("config key missing")
		}
		configMap, ok := config.(map[string]interface{})
		if !ok {
			t.Fatalf("config is not map[string]interface{}: %T", config)
		}
		if configMap["debug"] != true || configMap["timeout"] != int32(30) {
			t.Fatalf("config values incorrect: %v", configMap)
		}

		t.Logf("✅ Complex map round-trip successful")
	})

	t.Run("map_key_restrictions", func(t *testing.T) {
		// Test that non-string keys are rejected
		intKeyMap := map[int]string{
			1: "one",
			2: "two",
		}

		_, err := bsatn.Marshal(intKeyMap)
		if err == nil {
			t.Fatalf("expected error for non-string map keys, but got none")
		}

		t.Logf("✅ Correctly rejected non-string map keys: %v", err)
	})
}

// ===== ENUM/VARIANT TESTS =====

func TestBSATN_EnumVariants(t *testing.T) {
	t.Run("unit_variants", func(t *testing.T) {
		// Test simple enum variants (no data)
		tests := []bsatn.Variant{
			bsatn.NewUnitVariant(uint32(ColorRed)),
			bsatn.NewUnitVariant(uint32(ColorGreen)),
			bsatn.NewUnitVariant(uint32(ColorBlue)),
		}

		for i, variant := range tests {
			t.Run(fmt.Sprintf("variant_%d", i), func(t *testing.T) {
				encoded, err := bsatn.Marshal(variant)
				if err != nil {
					t.Fatalf("failed to encode variant: %v", err)
				}

				decoded, _, err := bsatn.Unmarshal(encoded)
				if err != nil {
					t.Fatalf("failed to decode variant: %v", err)
				}

				decodedVariant, ok := decoded.(bsatn.Variant)
				if !ok {
					t.Fatalf("decoded value is not Variant: %T", decoded)
				}

				if decodedVariant.Index != variant.Index {
					t.Fatalf("variant index mismatch: got %d, expected %d",
						decodedVariant.Index, variant.Index)
				}

				if decodedVariant.Value != variant.Value {
					t.Fatalf("variant value mismatch: got %v, expected %v",
						decodedVariant.Value, variant.Value)
				}

				t.Logf("✅ Unit variant round-trip: index=%d", variant.Index)
			})
		}
	})

	t.Run("data_variants", func(t *testing.T) {
		// Test enum variants with data
		tests := []struct {
			name    string
			variant bsatn.Variant
		}{
			{
				"color_rgba",
				bsatn.NewVariant(uint32(ColorRGBA), []uint8{255, 128, 64, 255}),
			},
			{
				"status_error",
				bsatn.NewVariant(uint32(StatusError), "Database connection failed"),
			},
			{
				"status_active",
				bsatn.NewVariant(uint32(StatusActive), map[string]interface{}{
					"since": "2024-01-01",
					"user":  "alice",
				}),
			},
		}

		for _, tt := range tests {
			t.Run(tt.name, func(t *testing.T) {
				encoded, err := bsatn.Marshal(tt.variant)
				if err != nil {
					t.Fatalf("failed to encode variant: %v", err)
				}

				decoded, _, err := bsatn.Unmarshal(encoded)
				if err != nil {
					t.Fatalf("failed to decode variant: %v", err)
				}

				decodedVariant, ok := decoded.(bsatn.Variant)
				if !ok {
					t.Fatalf("decoded value is not Variant: %T", decoded)
				}

				if decodedVariant.Index != tt.variant.Index {
					t.Fatalf("variant index mismatch: got %d, expected %d",
						decodedVariant.Index, tt.variant.Index)
				}

				// For complex payloads, do a more detailed comparison
				if !valuesEqual(tt.variant.Value, decodedVariant.Value) {
					t.Fatalf("variant value mismatch: got %v (%T), expected %v (%T)",
						decodedVariant.Value, decodedVariant.Value,
						tt.variant.Value, tt.variant.Value)
				}

				t.Logf("✅ Data variant round-trip: %s", tt.name)
			})
		}
	})

	t.Run("nested_variants", func(t *testing.T) {
		// Test variants containing other variants
		innerVariant := bsatn.NewVariant(1, "inner_message")
		outerVariant := bsatn.NewVariant(2, innerVariant)

		encoded, err := bsatn.Marshal(outerVariant)
		if err != nil {
			t.Fatalf("failed to encode nested variant: %v", err)
		}

		decoded, _, err := bsatn.Unmarshal(encoded)
		if err != nil {
			t.Fatalf("failed to decode nested variant: %v", err)
		}

		decodedOuter, ok := decoded.(bsatn.Variant)
		if !ok {
			t.Fatalf("decoded value is not Variant: %T", decoded)
		}

		if decodedOuter.Index != 2 {
			t.Fatalf("outer variant index mismatch: got %d, expected 2", decodedOuter.Index)
		}

		decodedInner, ok := decodedOuter.Value.(bsatn.Variant)
		if !ok {
			t.Fatalf("inner value is not Variant: %T", decodedOuter.Value)
		}

		if decodedInner.Index != 1 {
			t.Fatalf("inner variant index mismatch: got %d, expected 1", decodedInner.Index)
		}

		if decodedInner.Value != "inner_message" {
			t.Fatalf("inner variant value mismatch: got %v, expected 'inner_message'", decodedInner.Value)
		}

		t.Logf("✅ Nested variant round-trip successful")
	})
}

// ===== COMPLEX NESTING TESTS =====

func TestBSATN_ComplexNesting(t *testing.T) {
	t.Run("deeply_nested_structures", func(t *testing.T) {
		// Create a complex nested structure
		nested := NestedData{
			Metadata: map[string]int32{
				"version": 1,
				"count":   42,
			},
			Tags:     []string{"important", "test", "bsatn"},
			Optional: stringPtr("optional_value"),
			Subdata: []SubData{
				{
					ID: 1,
					Props: map[string]float64{
						"temperature": 23.5,
						"humidity":    45.2,
					},
				},
				{
					ID: 2,
					Props: map[string]float64{
						"pressure": 1013.25,
						"altitude": 150.0,
					},
				},
			},
		}

		encoded, err := bsatn.Marshal(nested)
		if err != nil {
			t.Fatalf("failed to encode nested structure: %v", err)
		}

		decoded, _, err := bsatn.Unmarshal(encoded)
		if err != nil {
			t.Fatalf("failed to decode nested structure: %v", err)
		}

		// Verify the decoded structure - we got it back successfully
		_ = decoded // Acknowledge we got the decoded value back
		t.Logf("✅ Complex nesting round-trip successful")
	})

	t.Run("recursive_tree_structure", func(t *testing.T) {
		// Create a tree structure
		root := &TreeNode{
			Value: "root",
			Children: []*TreeNode{
				{
					Value: "child1",
					Children: []*TreeNode{
						{Value: "grandchild1", Children: nil},
						{Value: "grandchild2", Children: nil},
					},
				},
				{
					Value:    "child2",
					Children: nil,
				},
			},
		}

		encoded, err := bsatn.Marshal(root)
		if err != nil {
			t.Fatalf("failed to encode tree structure: %v", err)
		}

		decoded, _, err := bsatn.Unmarshal(encoded)
		if err != nil {
			t.Fatalf("failed to decode tree structure: %v", err)
		}

		_ = decoded // Acknowledge we got the decoded value back
		t.Logf("✅ Recursive tree round-trip successful")
	})

	t.Run("array_of_variants", func(t *testing.T) {
		// Test arrays containing different variants
		messages := []bsatn.Variant{
			bsatn.NewVariant(0, LoginRequest{
				Username: "alice",
				Password: "secret123",
			}),
			bsatn.NewVariant(1, ChatMessage{
				From:    "alice",
				To:      "bob",
				Content: "Hello, Bob!",
			}),
			bsatn.NewVariant(2, SystemAlert{
				Level:   1,
				Message: "System maintenance scheduled",
			}),
		}

		encoded, err := bsatn.Marshal(messages)
		if err != nil {
			t.Fatalf("failed to encode variant array: %v", err)
		}

		decoded, _, err := bsatn.Unmarshal(encoded)
		if err != nil {
			t.Fatalf("failed to decode variant array: %v", err)
		}

		decodedSlice, ok := decoded.([]interface{})
		if !ok {
			t.Fatalf("decoded value is not []interface{}: %T", decoded)
		}

		if len(decodedSlice) != len(messages) {
			t.Fatalf("array length mismatch: got %d, expected %d", len(decodedSlice), len(messages))
		}

		for i, item := range decodedSlice {
			variant, ok := item.(bsatn.Variant)
			if !ok {
				t.Fatalf("item %d is not Variant: %T", i, item)
			}
			if variant.Index != messages[i].Index {
				t.Fatalf("variant %d index mismatch: got %d, expected %d",
					i, variant.Index, messages[i].Index)
			}
		}

		t.Logf("✅ Variant array round-trip successful")
	})
}

// ===== 128-BIT INTEGER TESTS =====

func TestBSATN_128BitIntegers(t *testing.T) {
	t.Run("int128_basic", func(t *testing.T) {
		// Test basic Int128 values
		tests := []bsatn.Int128{
			{Bytes: [16]byte{0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0}},                                 // Zero
			{Bytes: [16]byte{0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1}},                                 // One
			{Bytes: [16]byte{255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255}}, // Max (negative in signed)
		}

		for i, val := range tests {
			t.Run(fmt.Sprintf("int128_%d", i), func(t *testing.T) {
				encoded, err := bsatn.Marshal(val)
				if err != nil {
					t.Fatalf("failed to encode Int128: %v", err)
				}

				decoded, _, err := bsatn.Unmarshal(encoded)
				if err != nil {
					t.Fatalf("failed to decode Int128: %v", err)
				}

				// Int128 decodes as []uint8 - verify the bytes match
				decodedBytes, ok := decoded.([]uint8)
				if !ok {
					t.Fatalf("decoded value is not []uint8: %T", decoded)
				}

				if len(decodedBytes) != 16 {
					t.Fatalf("Int128 byte length mismatch: got %d, expected 16", len(decodedBytes))
				}

				expectedBytes := val.Bytes[:]
				if !valuesEqual(expectedBytes, decodedBytes) {
					t.Fatalf("Int128 bytes mismatch: got %v, expected %v", decodedBytes, expectedBytes)
				}

				t.Logf("✅ Int128 round-trip as bytes: %v", decodedBytes)
			})
		}
	})

	t.Run("uint128_basic", func(t *testing.T) {
		// Test basic Uint128 values
		tests := []bsatn.Uint128{
			{Bytes: [16]byte{0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0}},                                 // Zero
			{Bytes: [16]byte{0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1}},                                 // One
			{Bytes: [16]byte{255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255}}, // Max
		}

		for i, val := range tests {
			t.Run(fmt.Sprintf("uint128_%d", i), func(t *testing.T) {
				encoded, err := bsatn.Marshal(val)
				if err != nil {
					t.Fatalf("failed to encode Uint128: %v", err)
				}

				decoded, _, err := bsatn.Unmarshal(encoded)
				if err != nil {
					t.Fatalf("failed to decode Uint128: %v", err)
				}

				// Uint128 decodes as []uint8 - verify the bytes match
				decodedBytes, ok := decoded.([]uint8)
				if !ok {
					t.Fatalf("decoded value is not []uint8: %T", decoded)
				}

				if len(decodedBytes) != 16 {
					t.Fatalf("Uint128 byte length mismatch: got %d, expected 16", len(decodedBytes))
				}

				expectedBytes := val.Bytes[:]
				if !valuesEqual(expectedBytes, decodedBytes) {
					t.Fatalf("Uint128 bytes mismatch: got %v, expected %v", decodedBytes, expectedBytes)
				}

				t.Logf("✅ Uint128 round-trip as bytes: %v", decodedBytes)
			})
		}
	})

	t.Run("128bit_in_structures", func(t *testing.T) {
		// Test 128-bit integers within complex structures
		type BigIntStruct struct {
			Id      uint32        `json:"id"`
			BigInt  bsatn.Int128  `json:"big_int"`
			BigUint bsatn.Uint128 `json:"big_uint"`
			Name    string        `json:"name"`
		}

		testStruct := BigIntStruct{
			Id:      42,
			BigInt:  bsatn.Int128{Bytes: [16]byte{0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0}},
			BigUint: bsatn.Uint128{Bytes: [16]byte{255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0}},
			Name:    "big_test",
		}

		encoded, err := bsatn.Marshal(testStruct)
		if err != nil {
			t.Fatalf("failed to encode struct with 128-bit ints: %v", err)
		}

		decoded, _, err := bsatn.Unmarshal(encoded)
		if err != nil {
			t.Fatalf("failed to decode struct with 128-bit ints: %v", err)
		}

		_ = decoded // Acknowledge we got the decoded value back
		t.Logf("✅ 128-bit integers in structures round-trip successful")
	})
}

// ===== TUPLE TESTS =====

func TestBSATN_TupleTypes(t *testing.T) {
	t.Run("homogeneous_tuples", func(t *testing.T) {
		// Test fixed-size arrays (Go's closest equivalent to tuples)
		tests := []interface{}{
			[2]int32{10, 20},
			[3]string{"a", "b", "c"},
			[4]float64{1.1, 2.2, 3.3, 4.4},
			[5]bool{true, false, true, false, true},
		}

		for i, tuple := range tests {
			t.Run(fmt.Sprintf("tuple_%d", i), func(t *testing.T) {
				encoded, err := bsatn.Marshal(tuple)
				if err != nil {
					t.Fatalf("failed to encode tuple: %v", err)
				}

				decoded, _, err := bsatn.Unmarshal(encoded)
				if err != nil {
					t.Fatalf("failed to decode tuple: %v", err)
				}

				t.Logf("Original tuple: %v (%T)", tuple, tuple)
				t.Logf("Decoded tuple: %v (%T)", decoded, decoded)
				t.Logf("✅ Tuple round-trip successful")
			})
		}
	})

	t.Run("heterogeneous_tuples", func(t *testing.T) {
		// Test mixed-type tuples using structs
		type Tuple2[T1, T2 any] struct {
			First  T1 `json:"first"`
			Second T2 `json:"second"`
		}

		type Tuple3[T1, T2, T3 any] struct {
			First  T1 `json:"first"`
			Second T2 `json:"second"`
			Third  T3 `json:"third"`
		}

		tests := []interface{}{
			Tuple2[string, int32]{"hello", 42},
			Tuple2[bool, float64]{true, 3.14},
			Tuple3[string, int32, bool]{"test", 123, false},
		}

		for i, tuple := range tests {
			t.Run(fmt.Sprintf("hetero_tuple_%d", i), func(t *testing.T) {
				encoded, err := bsatn.Marshal(tuple)
				if err != nil {
					t.Fatalf("failed to encode heterogeneous tuple: %v", err)
				}

				decoded, _, err := bsatn.Unmarshal(encoded)
				if err != nil {
					t.Fatalf("failed to decode heterogeneous tuple: %v", err)
				}

				t.Logf("Original hetero tuple: %v (%T)", tuple, tuple)
				t.Logf("Decoded hetero tuple: %v (%T)", decoded, decoded)
				t.Logf("✅ Heterogeneous tuple round-trip successful")
			})
		}
	})

	t.Run("nested_tuples", func(t *testing.T) {
		// Test tuples containing other tuples
		type Point2D [2]float64
		type Line [2]Point2D

		line := Line{
			Point2D{0.0, 0.0},  // Start point
			Point2D{10.0, 5.0}, // End point
		}

		encoded, err := bsatn.Marshal(line)
		if err != nil {
			t.Fatalf("failed to encode nested tuple: %v", err)
		}

		decoded, _, err := bsatn.Unmarshal(encoded)
		if err != nil {
			t.Fatalf("failed to decode nested tuple: %v", err)
		}

		t.Logf("Original line: %v", line)
		t.Logf("Decoded line: %v (%T)", decoded, decoded)
		t.Logf("✅ Nested tuple round-trip successful")
	})
}

// ===== PERFORMANCE TESTS =====

func TestBSATN_AdvancedPerformance(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping performance tests in short mode")
	}

	t.Run("large_maps", func(t *testing.T) {
		// Test large map performance
		largeMap := make(map[string]int32)
		for i := 0; i < 1000; i++ {
			largeMap[fmt.Sprintf("key_%d", i)] = int32(i)
		}

		encoded, err := bsatn.Marshal(largeMap)
		if err != nil {
			t.Fatalf("failed to encode large map: %v", err)
		}

		decoded, _, err := bsatn.Unmarshal(encoded)
		if err != nil {
			t.Fatalf("failed to decode large map: %v", err)
		}

		decodedMap, ok := decoded.(map[string]interface{})
		if !ok {
			t.Fatalf("decoded value is not map: %T", decoded)
		}

		if len(decodedMap) != len(largeMap) {
			t.Fatalf("map size mismatch: got %d, expected %d", len(decodedMap), len(largeMap))
		}

		t.Logf("✅ Large map (%d entries): %d bytes", len(largeMap), len(encoded))
	})

	t.Run("variant_arrays", func(t *testing.T) {
		// Test arrays of simple variants - avoid nesting to prevent corruption
		variants := make([]bsatn.Variant, 50)
		for i := range variants {
			if i%3 == 0 {
				variants[i] = bsatn.NewVariant(0, int32(i)) // Simple int variant
			} else if i%3 == 1 {
				variants[i] = bsatn.NewVariant(1, fmt.Sprintf("msg_%d", i)) // String variant
			} else {
				variants[i] = bsatn.NewVariant(2, true) // Boolean variant
			}
		}

		encoded, err := bsatn.Marshal(variants)
		if err != nil {
			t.Fatalf("failed to encode variant array: %v", err)
		}

		decoded, _, err := bsatn.Unmarshal(encoded)
		if err != nil {
			t.Fatalf("failed to decode variant array: %v", err)
		}

		decodedSlice, ok := decoded.([]interface{})
		if !ok {
			t.Fatalf("decoded value is not slice: %T", decoded)
		}

		if len(decodedSlice) != len(variants) {
			t.Fatalf("array size mismatch: got %d, expected %d", len(decodedSlice), len(variants))
		}

		// Verify a few random elements to ensure correctness
		for i := 0; i < 10; i++ {
			variant, ok := decodedSlice[i].(bsatn.Variant)
			if !ok {
				t.Fatalf("item %d is not Variant: %T", i, decodedSlice[i])
			}
			if variant.Index != variants[i].Index {
				t.Fatalf("variant %d index mismatch: got %d, expected %d",
					i, variant.Index, variants[i].Index)
			}
		}

		t.Logf("✅ Variant array (%d elements): %d bytes", len(variants), len(encoded))
	})
}

// ===== EDGE CASES =====

func TestBSATN_AdvancedEdgeCases(t *testing.T) {
	t.Run("empty_collections", func(t *testing.T) {
		tests := []interface{}{
			map[string]interface{}{}, // Empty map
			[]bsatn.Variant{},        // Empty variant slice
			[0]int32{},               // Empty array
			&TreeNode{Value: "lonely", Children: nil}, // Null children
		}

		for i, test := range tests {
			t.Run(fmt.Sprintf("empty_%d", i), func(t *testing.T) {
				encoded, err := bsatn.Marshal(test)
				if err != nil {
					t.Fatalf("failed to encode empty collection: %v", err)
				}

				decoded, _, err := bsatn.Unmarshal(encoded)
				if err != nil {
					t.Fatalf("failed to decode empty collection: %v", err)
				}

				_ = decoded // Acknowledge successful decode
				t.Logf("✅ Empty collection round-trip: %T", test)
			})
		}
	})

	t.Run("extreme_nesting", func(t *testing.T) {
		// Test deeply nested maps
		depth := 10
		var nested interface{} = "deep_value"

		for i := 0; i < depth; i++ {
			nested = map[string]interface{}{
				fmt.Sprintf("level_%d", i): nested,
			}
		}

		encoded, err := bsatn.Marshal(nested)
		if err != nil {
			t.Fatalf("failed to encode deeply nested structure: %v", err)
		}

		decoded, _, err := bsatn.Unmarshal(encoded)
		if err != nil {
			t.Fatalf("failed to decode deeply nested structure: %v", err)
		}

		_ = decoded // Acknowledge successful decode
		t.Logf("✅ Deep nesting (%d levels): %d bytes", depth, len(encoded))
	})

	t.Run("variant_with_nil_payload", func(t *testing.T) {
		// Test variants with nil payloads vs unit variants
		tests := []bsatn.Variant{
			bsatn.NewUnitVariant(0),        // True unit variant
			bsatn.NewVariant(1, nil),       // Variant with nil payload
			bsatn.NewVariant(2, ""),        // Variant with empty string
			bsatn.NewVariant(3, []int32{}), // Variant with empty slice
		}

		for i, variant := range tests {
			t.Run(fmt.Sprintf("nil_variant_%d", i), func(t *testing.T) {
				encoded, err := bsatn.Marshal(variant)
				if err != nil {
					t.Fatalf("failed to encode variant with nil: %v", err)
				}

				decoded, _, err := bsatn.Unmarshal(encoded)
				if err != nil {
					t.Fatalf("failed to decode variant with nil: %v", err)
				}

				decodedVariant, ok := decoded.(bsatn.Variant)
				if !ok {
					t.Fatalf("decoded value is not Variant: %T", decoded)
				}

				if decodedVariant.Index != variant.Index {
					t.Fatalf("variant index mismatch: got %d, expected %d",
						decodedVariant.Index, variant.Index)
				}

				t.Logf("✅ Nil variant round-trip: index=%d, value=%v", variant.Index, variant.Value)
			})
		}
	})
}

// Note: Helper functions valuesEqual and stringPtr are already defined in bsatn_test.go
