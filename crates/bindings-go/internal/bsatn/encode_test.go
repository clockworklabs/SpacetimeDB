package bsatn

import (
	"reflect"
	"testing"
)

func TestRoundTripPrimitives(t *testing.T) {
	cases := []interface{}{true, false, uint8(42), int8(-5), uint16(65500), int16(-1234), uint32(4000000000), int32(-2000000000), uint64(1 << 60), int64(-1 << 50), float32(3.14), float64(-6.28), "hello", []byte{0x01, 0x02, 0x03}}

	// add list and option
	cases = append(cases, []interface{}{uint8(1), uint8(2)}, (*int)(nil))
	cases = append(cases, struct {
		A uint8
		B string
	}{A: 5, B: "str"})

	// struct with bsatn tags
	type S struct {
		ID  uint8 `bsatn:"id"`
		Ign int   `bsatn:"-"`
	}
	cases = append(cases, S{ID: 9, Ign: 1})

	// enum variants
	cases = append(cases, NewUnitVariant(3))
	cases = append(cases, NewVariant(1, uint8(7)))

	// large integer byte arrays
	var u128Bytes [16]byte
	for i := range u128Bytes {
		u128Bytes[i] = byte(i + 1)
	}
	var u256Bytes [32]byte
	for i := range u256Bytes {
		u256Bytes[i] = byte(i + 101)
	}
	cases = append(cases, u128Bytes, u256Bytes)

	for _, c := range cases {
		encoded, err := Marshal(c)
		if err != nil {
			t.Fatalf("marshal %v: %v", c, err)
		}
		decoded, _, err := Unmarshal(encoded)
		if err != nil {
			t.Fatalf("unmarshal %v: %v", c, err)
		}
		if _, ok := c.(Variant); ok {
			// compare variant index and payload via reflect.DeepEqual
			vexp := c.(Variant)
			vgot := decoded.(Variant)
			if vexp.Index != vgot.Index || !reflect.DeepEqual(vexp.Value, vgot.Value) {
				t.Fatalf("variant mismatch exp=%v got=%v", vexp, vgot)
			}
			continue
		}

		if reflect.ValueOf(c).Kind() == reflect.Struct {
			// If this is the tagged struct type, verify tag handling
			if reflect.TypeOf(c).Name() == "S" {
				m := decoded.(map[string]interface{})
				if _, ok := m["id"]; !ok {
					t.Fatalf("tagged struct missing id field")
				}
				var out S
				if err := UnmarshalInto(encoded, &out); err != nil {
					t.Fatalf("unmarshal into struct failed: %v", err)
				}
				if out.ID != uint8(9) {
					t.Fatalf("struct decode mismatch")
				}
			}
			continue
		}

		equal := false
		if b, ok := c.([]byte); ok {
			if decSlice, okDec := decoded.([]byte); okDec {
				equal = reflect.DeepEqual(b, decSlice)
			} else {
				t.Fatalf("decoded type mismatch for []byte: expected []byte, got %T for original %#v", decoded, c)
			}
		} else if arr16, ok := c.([16]byte); ok {
			if decSlice, okDec := decoded.([]byte); okDec {
				if len(decSlice) != 16 {
					t.Fatalf("decoded []byte length mismatch for [16]byte: got %d, want 16", len(decSlice))
				}
				var originalSlice [16]byte = arr16
				equal = reflect.DeepEqual(originalSlice[:], decSlice)
			} else {
				t.Fatalf("decoded type mismatch for [16]byte: expected []byte, got %T for original %#v", decoded, c)
			}
		} else if arr32, ok := c.([32]byte); ok {
			if decSlice, okDec := decoded.([]byte); okDec {
				if len(decSlice) != 32 {
					t.Fatalf("decoded []byte length mismatch for [32]byte: got %d, want 32", len(decSlice))
				}
				var originalSlice [32]byte = arr32
				equal = reflect.DeepEqual(originalSlice[:], decSlice)
			} else {
				t.Fatalf("decoded type mismatch for [32]byte: expected []byte, got %T for original %#v", decoded, c)
			}
		} else {
			rv := reflect.ValueOf(c)
			if rv.Kind() == reflect.Ptr && rv.IsNil() {
				equal = decoded == nil
			} else {
				equal = reflect.DeepEqual(decoded, c)
			}
		}

		if !equal {
			t.Fatalf("round-trip mismatch: got %#v (%T) want %#v (%T)", decoded, decoded, c, c)
		}
	}
}
