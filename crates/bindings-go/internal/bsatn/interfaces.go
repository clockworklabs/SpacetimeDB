package bsatn

// IStructuralReadWrite defines the interface for types that can control
// their own BSATN serialization and deserialization.
// This is an alternative to relying on reflection-based struct handling.
type IStructuralReadWrite interface {
	// WriteBSATN encodes the receiver into BSATN format using the provided Writer.
	WriteBSATN(writer *Writer) error

	// ReadBSATN decodes BSATN data from the provided Reader into the receiver.
	// The receiver should be a pointer to the type being decoded.
	ReadBSATN(reader *Reader) error

	// TODO: Consider adding these in the future if deemed necessary:
	// SizeBSATN() (int, error) // Estimates the size of the BSATN encoded form.
	// ValidateBSATN() error      // Validates the struct against its schema/constraints.
}
