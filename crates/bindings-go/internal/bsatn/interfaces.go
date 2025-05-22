package bsatn

// IStructuralReadWrite defines the minimal contract for a type that wants to
// bypass reflection and handle its own BSATN encoding/decoding.
//
// Additional optional capabilities such as sizing and validation are expressed
// through separate, composable interfaces so existing user types remain
// compatible without changes.
type IStructuralReadWrite interface {
	// WriteBSATN encodes the receiver into BSATN format using the provided Writer.
	WriteBSATN(writer *Writer) error

	// ReadBSATN decodes BSATN data from the provided Reader into the receiver.
	// The receiver should be a pointer to the type being decoded.
	ReadBSATN(reader *Reader) error
}

// IStructuralSizer is an optional companion interface. When implemented it
// should return the exact number of bytes that WriteBSATN will emit. Returning
// (-1, nil) indicates that the size cannot be determined cheaply in advance.
type IStructuralSizer interface {
	SizeBSATN() (int, error)
}

// IStructuralValidator is another optional companion interface for custom
// semantic checks before serialization or after deserialization.
type IStructuralValidator interface {
	ValidateBSATN() error
}

// IReadWrite is a generic helper interface for standalone serializers that are
// not bound to a concrete Go type.  Instead, the methods operate on values of
// the generic parameter T.
//
//	var u8Codec IReadWrite[uint8] = bsatn.Uint8Codec{}
//	n, _ := u8Codec.Size(42)
//
// The interface mirrors the IStructuralReadWrite capabilities but with the
// value passed explicitly.
type IReadWrite[T any] interface {
	// Write encodes the given value into BSATN using the provided Writer.
	Write(writer *Writer, val T) error

	// Read decodes a value of type T from the provided Reader.
	Read(reader *Reader) (T, error)

	// Size returns the encoded size in bytes of the provided value.
	Size(val T) (int, error)

	// Validate performs custom validation on the value prior to writing.
	Validate(val T) error
}
