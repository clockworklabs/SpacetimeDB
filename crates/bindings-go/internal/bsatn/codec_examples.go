package bsatn

import (
	"fmt"
)

// Vec2 is an example custom type implementing full structural interfaces.
// It serialises as two TagI32 payloads (X then Y).
// It is considered valid when |X| and |Y| <= 1000.
// Encoded size is constant: each int32 is TagI32 (1 byte) + 4 payload bytes = 5; total 10.
type Vec2 struct {
	X int32
	Y int32
}

// WriteBSATN implements IStructuralReadWrite.
func (v *Vec2) WriteBSATN(w *Writer) error {
	// Encode X
	w.WriteInt32(v.X)
	// Encode Y
	w.WriteInt32(v.Y)
	return w.Error()
}

// ReadBSATN implements IStructuralReadWrite.
func (v *Vec2) ReadBSATN(r *Reader) error {
	// Expect tag for X
	tag, err := r.ReadTag()
	if err != nil {
		return err
	}
	if tag != TagI32 {
		return fmt.Errorf("Vec2.ReadBSATN: expected TagI32 for X, got 0x%x", tag)
	}
	v.X, err = r.ReadInt32()
	if err != nil {
		return err
	}
	// Expect tag for Y
	tag, err = r.ReadTag()
	if err != nil {
		return err
	}
	if tag != TagI32 {
		return fmt.Errorf("Vec2.ReadBSATN: expected TagI32 for Y, got 0x%x", tag)
	}
	v.Y, err = r.ReadInt32()
	if err != nil {
		return err
	}
	return nil
}

// SizeBSATN implements IStructuralSizer.
func (v *Vec2) SizeBSATN() (int, error) {
	return 10, nil // two int32 with tag each
}

// ValidateBSATN implements IStructuralValidator.
func (v *Vec2) ValidateBSATN() error {
	if v.X > 1000 || v.X < -1000 || v.Y > 1000 || v.Y < -1000 {
		return fmt.Errorf("Vec2 out of allowed range (-1000..1000): (%d,%d)", v.X, v.Y)
	}
	return nil
}

// Uint8Codec is a simple IReadWrite implementation for uint8 values.
type Uint8Codec struct{}

func (Uint8Codec) Write(w *Writer, val uint8) error {
	w.WriteUint8(val)
	return w.Error()
}

func (Uint8Codec) Read(r *Reader) (uint8, error) {
	// Expect tag first
	tag, err := r.ReadTag()
	if err != nil {
		return 0, err
	}
	if tag != TagU8 {
		return 0, fmt.Errorf("Uint8Codec.Read: expected TagU8, got 0x%x", tag)
	}
	return r.ReadUint8()
}

func (Uint8Codec) Size(val uint8) (int, error) { return 2, nil }

func (Uint8Codec) Validate(val uint8) error {
	if val > 200 {
		return fmt.Errorf("Uint8Codec: value %d exceeds allowed maximum 200", val)
	}
	return nil
}
