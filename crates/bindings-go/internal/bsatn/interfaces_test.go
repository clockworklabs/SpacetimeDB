package bsatn

import (
	"bytes"
	"testing"
)

func TestVec2StructuralInterfaces(t *testing.T) {
	v := &Vec2{X: 123, Y: -456}

	// Validate should succeed
	if err := v.ValidateBSATN(); err != nil {
		t.Fatalf("unexpected ValidateBSATN error: %v", err)
	}

	// Size should be constant 10
	sz, err := v.SizeBSATN()
	if err != nil || sz != 10 {
		t.Fatalf("SizeBSATN mismatch got (%d,%v), want (10,nil)", sz, err)
	}

	// Round-trip via Marshal / UnmarshalInto
	enc, err := Marshal(v)
	if err != nil {
		t.Fatalf("Marshal failed: %v", err)
	}
	if len(enc) != sz {
		t.Fatalf("encoded length mismatch: got %d, want %d", len(enc), sz)
	}

	var v2 Vec2
	if err := UnmarshalInto(enc, &v2); err != nil {
		t.Fatalf("UnmarshalInto failed: %v", err)
	}
	if v2 != *v {
		t.Fatalf("round-trip mismatch: got %+v, want %+v", v2, *v)
	}

	// Validation failure case
	bad := &Vec2{X: 2000, Y: 0}
	if err := bad.ValidateBSATN(); err == nil {
		t.Fatal("expected validation error for out-of-range Vec2, got nil")
	}
}

func TestUint8Codec(t *testing.T) {
	codec := Uint8Codec{}
	var buf bytes.Buffer
	w := NewWriter(&buf)
	if err := codec.Write(w, 42); err != nil {
		t.Fatalf("codec.Write failed: %v", err)
	}
	if w.Error() != nil {
		t.Fatalf("writer error: %v", w.Error())
	}
	enc := buf.Bytes()
	if len(enc) != 2 {
		t.Fatalf("encoded length mismatch got %d, want 2", len(enc))
	}
	if sz, _ := codec.Size(42); sz != 2 {
		t.Fatalf("Size() mismatch got %d, want 2", sz)
	}

	// Decode
	r := NewReader(bytes.NewReader(enc))
	val, err := codec.Read(r)
	if err != nil {
		t.Fatalf("codec.Read failed: %v", err)
	}
	if val != 42 {
		t.Fatalf("decoded value mismatch got %d, want 42", val)
	}

	// Validate success/failure
	if err := codec.Validate(199); err != nil {
		t.Fatalf("unexpected validate error: %v", err)
	}
	if err := codec.Validate(201); err == nil {
		t.Fatal("expected validation error for 201, got nil")
	}
}

func TestInvalidFloatHandling(t *testing.T) {
	// existing code...
}

func TestMaxPayloadLen(t *testing.T) {
	big := make([]byte, MaxPayloadLen+1)
	// Writer should error
	var buf bytes.Buffer
	w := NewWriter(&buf)
	w.WriteString(string(big))
	if w.Error() != ErrTooLarge {
		t.Fatalf("expected ErrTooLarge when writing long string, got %v", w.Error())
	}

	// Reader should error
	// craft TagString + length (u32) with large size but no data to keep memory low
	tmp := bytes.Buffer{}
	tmp.WriteByte(TagString)
	// little endian uint32
	size := uint32(MaxPayloadLen + 1)
	tmp.Write([]byte{byte(size), byte(size >> 8), byte(size >> 16), byte(size >> 24)})
	r := NewReader(bytes.NewReader(tmp.Bytes()))
	tag, _ := r.ReadTag()
	if tag != TagString {
		t.Fatalf("setup fail")
	}
	if _, err := r.ReadString(); err != ErrTooLarge {
		t.Fatalf("expected ErrTooLarge from reader, got %v", err)
	}
}

func TestWriterReaderPosition(t *testing.T) {
	var buf bytes.Buffer
	w := NewWriter(&buf)
	w.WriteUint8(7)
	w.WriteString("hi")
	if w.Error() != nil {
		t.Fatalf("writer error: %v", w.Error())
	}
	written := w.BytesWritten()
	if written != buf.Len() {
		t.Fatalf("BytesWritten mismatch got %d want %d", written, buf.Len())
	}

	r := NewReader(bytes.NewReader(buf.Bytes()))
	tag, _ := r.ReadTag()
	if tag != TagU8 {
		t.Fatal("bad tag")
	}
	r.ReadUint8()
	tag, _ = r.ReadTag()
	if tag != TagString {
		t.Fatal("bad tag2")
	}
	r.ReadString()
	if r.BytesRead() != written {
		t.Fatalf("BytesRead mismatch got %d want %d", r.BytesRead(), written)
	}
}
