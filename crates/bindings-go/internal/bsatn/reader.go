package bsatn

import (
	"encoding/binary"
	"io"
	"math"
)

// Reader helps in decoding BSATN-encoded data into Go values.
// It wraps an io.Reader (typically a bytes.Reader or bytes.Buffer) and provides
// methods for reading various BSATN-tagged primitive types and structures.
// It keeps track of bytes consumed and errors.
type Reader struct {
	r         io.Reader
	bytesRead int   // Total bytes read from the underlying reader successfully by Reader methods
	err       error // Stores the first error encountered during reading

	// For byte-limited reading, e.g. when a payload has a known size
	limit      int // Max bytes to read for current operation; -1 for no limit
	limitStart int // bytesRead at the start of the limited operation
}

// NewReader creates a new BSATN Reader that reads from the provided io.Reader.
// A bytes.Reader or *bytes.Buffer is commonly used as the underlying reader.
func NewReader(r io.Reader) *Reader {
	return &Reader{r: r, limit: -1}
}

// Error returns the first error that occurred during reading, if any.
func (r *Reader) Error() error {
	return r.err
}

// BytesRead returns the total number of bytes successfully processed from the reader by Reader methods.
func (r *Reader) BytesRead() int {
	return r.bytesRead
}

// Remaining returns the number of bytes left before the current limit is
// reached. If no limit is active it returns -1.
func (r *Reader) Remaining() int {
	if r.limit == -1 {
		return -1
	}
	consumed := r.bytesRead - r.limitStart
	return r.limit - consumed
}

// recordError records the first error encountered.
// It also stops further reading if an error occurs by setting a limit.
func (r *Reader) recordError(err error) {
	if r.err == nil && err != nil {
		r.err = err
		// Prevent further reads by setting a limit that's already met
		r.limit = r.bytesRead
		r.limitStart = r.bytesRead
	}
}

// readByte reads a single byte.
func (r *Reader) readByte() (byte, error) {
	if r.err != nil {
		return 0, r.err
	}
	if r.limit != -1 && (r.bytesRead-r.limitStart) >= r.limit {
		r.recordError(io.EOF) // Or a more specific "limit reached" error
		return 0, r.err
	}
	var b [1]byte
	n, err := io.ReadFull(r.r, b[:])
	r.bytesRead += n
	if err != nil {
		r.recordError(err)
		return 0, err
	}
	return b[0], nil
}

// readBytes reads exactly len(p) bytes into p.
func (r *Reader) readBytes(p []byte) error {
	if r.err != nil {
		return r.err
	}
	lenP := len(p)
	if r.limit != -1 && (r.bytesRead-r.limitStart)+lenP > r.limit {
		r.recordError(io.ErrUnexpectedEOF) // Not enough bytes remaining within limit
		return r.err
	}
	n, err := io.ReadFull(r.r, p)
	r.bytesRead += n
	if err != nil {
		r.recordError(err)
	}
	return err
}

// --- Methods for reading BSATN tags and data will be added below ---

// ReadTag reads and returns the next BSATN tag byte.
func (r *Reader) ReadTag() (byte, error) {
	tag, err := r.readByte()
	if err != nil {
		// Don't record error here as readByte already does
		return 0, err
	}
	return tag, nil
}

// ReadBool decodes and returns a boolean value.
func (r *Reader) ReadBool(tag byte) (bool, error) { // Expects tag to be pre-read
	if r.err != nil {
		return false, r.err
	}
	switch tag {
	case TagBoolFalse:
		return false, nil
	case TagBoolTrue:
		return true, nil
	default:
		r.recordError(ErrInvalidTag)
		return false, r.err
	}
}

// ReadUint8 decodes and returns a uint8 value.
func (r *Reader) ReadUint8() (uint8, error) {
	// TagU8 should have been read by caller to dispatch here, or use ReadTag then this.
	// This function assumes the tag was already consumed and we are reading payload.
	val, err := r.readByte()
	if err != nil {
		return 0, err
	}
	return val, nil
}

// ReadInt8 decodes and returns an int8 value.
func (r *Reader) ReadInt8() (int8, error) {
	val, err := r.readByte()
	if err != nil {
		return 0, err
	}
	return int8(val), nil
}

// readUint16LE reads a uint16 in little-endian.
func (r *Reader) readUint16LE() (uint16, error) {
	if r.err != nil {
		return 0, r.err
	}
	var buf [2]byte
	err := r.readBytes(buf[:])
	if err != nil {
		return 0, err
	}
	return binary.LittleEndian.Uint16(buf[:]), nil
}

// ReadUint16 decodes and returns a uint16 value.
func (r *Reader) ReadUint16() (uint16, error) {
	return r.readUint16LE()
}

// ReadInt16 decodes and returns an int16 value.
func (r *Reader) ReadInt16() (int16, error) {
	v, err := r.readUint16LE()
	return int16(v), err
}

// readUint32LE reads a uint32 in little-endian.
func (r *Reader) readUint32LE() (uint32, error) {
	if r.err != nil {
		return 0, r.err
	}
	var buf [4]byte
	err := r.readBytes(buf[:])
	if err != nil {
		return 0, err
	}
	return binary.LittleEndian.Uint32(buf[:]), nil
}

// ReadUint32 decodes and returns a uint32 value.
func (r *Reader) ReadUint32() (uint32, error) {
	return r.readUint32LE()
}

// ReadInt32 decodes and returns an int32 value.
func (r *Reader) ReadInt32() (int32, error) {
	v, err := r.readUint32LE()
	return int32(v), err
}

// readUint64LE reads a uint64 in little-endian.
func (r *Reader) readUint64LE() (uint64, error) {
	if r.err != nil {
		return 0, r.err
	}
	var buf [8]byte
	err := r.readBytes(buf[:])
	if err != nil {
		return 0, err
	}
	return binary.LittleEndian.Uint64(buf[:]), nil
}

// ReadUint64 decodes and returns a uint64 value.
func (r *Reader) ReadUint64() (uint64, error) {
	return r.readUint64LE()
}

// ReadInt64 decodes and returns an int64 value.
func (r *Reader) ReadInt64() (int64, error) {
	v, err := r.readUint64LE()
	return int64(v), err
}

// ReadFloat32 decodes and returns a float32 value.
func (r *Reader) ReadFloat32() (float32, error) {
	bits, err := r.readUint32LE()
	if err != nil {
		return 0, err
	}
	v := math.Float32frombits(bits)
	if math.IsNaN(float64(v)) || math.IsInf(float64(v), 0) {
		r.recordError(ErrInvalidFloat)
		return 0, r.err
	}
	return v, nil
}

// ReadFloat64 decodes and returns a float64 value.
func (r *Reader) ReadFloat64() (float64, error) {
	bits, err := r.readUint64LE()
	if err != nil {
		return 0, err
	}
	v := math.Float64frombits(bits)
	if math.IsNaN(v) || math.IsInf(v, 0) {
		r.recordError(ErrInvalidFloat)
		return 0, r.err
	}
	return v, nil
}

// ReadString decodes and returns a string value.
// Assumes TagString was already read and confirmed by caller.
func (r *Reader) ReadString() (string, error) {
	if r.err != nil {
		return "", r.err
	}
	size, err := r.readUint32LE() // Read length prefix
	if err != nil {
		return "", err
	}
	if size == 0 {
		return "", nil
	}
	if int(size) > MaxPayloadLen {
		r.recordError(ErrTooLarge)
		return "", r.err
	}
	data := make([]byte, size)
	err = r.readBytes(data)
	if err != nil {
		return "", err
	}
	// TODO: UTF-8 validation if strict
	return string(data), nil
}

// ReadBytes decodes and returns a byte slice.
// Assumes TagBytes was already read and confirmed by caller.
func (r *Reader) ReadBytesRaw() ([]byte, error) { // Renamed to avoid conflict with internal readBytes
	if r.err != nil {
		return nil, r.err
	}
	size, err := r.readUint32LE() // Read length prefix
	if err != nil {
		return nil, err
	}
	if size == 0 {
		return []byte{}, nil
	}
	if int(size) > MaxPayloadLen {
		r.recordError(ErrTooLarge)
		return nil, r.err
	}
	// TODO: Add check against max slice length
	data := make([]byte, size)
	err = r.readBytes(data)
	if err != nil {
		return nil, err
	}
	return data, nil
}

// ReadU128Bytes decodes and returns 16 bytes for U128.
func (r *Reader) ReadU128Bytes() ([]byte, error) {
	data := make([]byte, 16)
	err := r.readBytes(data)
	return data, err
}

// ReadI128Bytes decodes and returns 16 bytes for I128.
func (r *Reader) ReadI128Bytes() ([]byte, error) {
	data := make([]byte, 16)
	err := r.readBytes(data)
	return data, err
}

// ReadU256Bytes decodes and returns 32 bytes for U256.
func (r *Reader) ReadU256Bytes() ([]byte, error) {
	data := make([]byte, 32)
	err := r.readBytes(data)
	return data, err
}

// ReadI256Bytes decodes and returns 32 bytes for I256.
func (r *Reader) ReadI256Bytes() ([]byte, error) {
	data := make([]byte, 32)
	err := r.readBytes(data)
	return data, err
}

// ReadListHeader reads and returns the count of items for a list.
// Assumes TagList was already read.
func (r *Reader) ReadListHeader() (count uint32, err error) {
	if r.err != nil {
		return 0, r.err
	}
	return r.readUint32LE()
}

// ReadArrayHeader reads and returns the count of items for an array.
// Assumes TagArray was already read.
func (r *Reader) ReadArrayHeader() (count uint32, err error) {
	if r.err != nil {
		return 0, r.err
	}
	return r.readUint32LE()
}

// ReadStructHeader reads and returns the field count for a struct.
// Assumes TagStruct was already read.
func (r *Reader) ReadStructHeader() (fieldCount uint32, err error) {
	if r.err != nil {
		return 0, r.err
	}
	return r.readUint32LE()
}

// ReadFieldName reads and returns a field name for a struct.
func (r *Reader) ReadFieldName() (string, error) {
	if r.err != nil {
		return "", r.err
	}
	nameLen, err := r.readByte() // nameLen is u8
	if err != nil {
		return "", err
	}
	if nameLen == 0 {
		return "", nil
	}
	nameBytes := make([]byte, nameLen)
	err = r.readBytes(nameBytes)
	if err != nil {
		return "", err
	}
	// TODO: UTF-8 validation if strict
	return string(nameBytes), nil
}

// ReadEnumHeader reads and returns the variant index for an enum.
// Assumes TagEnum was already read.
func (r *Reader) ReadEnumHeader() (variantIndex uint32, err error) {
	if r.err != nil {
		return 0, r.err
	}
	return r.readUint32LE()
}
