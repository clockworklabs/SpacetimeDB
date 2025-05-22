package bsatn

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"io"
	"math"
	"unicode/utf8"
)

// Writer helps in encoding Go values into BSATN format.
// It wraps an io.Writer (typically a bytes.Buffer) and provides methods
// for writing various BSATN-tagged primitive types and structures.
type Writer struct {
	w            io.Writer
	err          error // Stores the first error encountered during writing.
	bytesWritten int   // number of bytes successfully written to underlying writer
}

// NewWriter creates a new BSATN Writer that writes to the provided io.Writer.
// A bytes.Buffer is commonly used as the underlying writer.
func NewWriter(w io.Writer) *Writer {
	return &Writer{w: w}
}

// Bytes returns the written bytes if the underlying writer is a *bytes.Buffer.
// It returns nil if the writer is not a *bytes.Buffer or if an error occurred.
func (w *Writer) Bytes() []byte {
	if w.err != nil {
		return nil
	}
	if bb, ok := w.w.(*bytes.Buffer); ok {
		return bb.Bytes()
	}
	return nil
}

// Error returns the first error that occurred during writing, if any.
func (w *Writer) Error() error {
	return w.err
}

// BytesWritten returns the number of bytes that have been successfully written
// via this Writer so far, or -1 if an error occurred before any write.
func (w *Writer) BytesWritten() int {
	return w.bytesWritten
}

// recordError records the first error encountered.
func (w *Writer) recordError(err error) {
	if w.err == nil && err != nil {
		w.err = err
	}
}

// --- Methods for writing BSATN tags and data will be added below ---

// WriteTag directly writes a BSATN tag byte.
func (w *Writer) WriteTag(tag byte) {
	if w.err != nil {
		return
	}
	_, err := w.w.Write([]byte{tag})
	if err == nil {
		w.bytesWritten++
	}
	w.recordError(err)
}

// WriteBool encodes and writes a boolean value.
func (w *Writer) WriteBool(val bool) {
	if w.err != nil {
		return
	}
	if val {
		w.WriteTag(TagBoolTrue)
	} else {
		w.WriteTag(TagBoolFalse)
	}
}

// WriteUint8 encodes and writes a uint8 value.
func (w *Writer) WriteUint8(val uint8) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagU8)
	_, err := w.w.Write([]byte{val})
	if err == nil {
		w.bytesWritten++
	}
	w.recordError(err)
}

// WriteInt8 encodes and writes an int8 value.
func (w *Writer) WriteInt8(val int8) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagI8)
	_, err := w.w.Write([]byte{byte(val)})
	if err == nil {
		w.bytesWritten++
	}
	w.recordError(err)
}

// writeUint16LE writes a uint16 in little-endian format.
func (w *Writer) writeUint16LE(val uint16) {
	if w.err != nil {
		return
	}
	var buf [2]byte
	binary.LittleEndian.PutUint16(buf[:], val)
	_, err := w.w.Write(buf[:])
	if err == nil {
		w.bytesWritten += 2
	}
	w.recordError(err)
}

// WriteUint16 encodes and writes a uint16 value.
func (w *Writer) WriteUint16(val uint16) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagU16)
	w.writeUint16LE(val)
}

// WriteInt16 encodes and writes an int16 value.
func (w *Writer) WriteInt16(val int16) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagI16)
	w.writeUint16LE(uint16(val)) // Cast and write as uint16
}

// writeUint32LE writes a uint32 in little-endian format.
func (w *Writer) writeUint32LE(val uint32) {
	if w.err != nil {
		return
	}
	var buf [4]byte
	binary.LittleEndian.PutUint32(buf[:], val)
	_, err := w.w.Write(buf[:])
	if err == nil {
		w.bytesWritten += 4
	}
	w.recordError(err)
}

// WriteUint32 encodes and writes a uint32 value.
func (w *Writer) WriteUint32(val uint32) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagU32)
	w.writeUint32LE(val)
}

// WriteInt32 encodes and writes an int32 value.
func (w *Writer) WriteInt32(val int32) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagI32)
	w.writeUint32LE(uint32(val)) // Cast and write as uint32
}

// writeUint64LE writes a uint64 in little-endian format.
func (w *Writer) writeUint64LE(val uint64) {
	if w.err != nil {
		return
	}
	var buf [8]byte
	binary.LittleEndian.PutUint64(buf[:], val)
	_, err := w.w.Write(buf[:])
	if err == nil {
		w.bytesWritten += 8
	}
	w.recordError(err)
}

// WriteUint64 encodes and writes a uint64 value.
func (w *Writer) WriteUint64(val uint64) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagU64)
	w.writeUint64LE(val)
}

// WriteInt64 encodes and writes an int64 value.
func (w *Writer) WriteInt64(val int64) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagI64)
	w.writeUint64LE(uint64(val)) // Cast and write as uint64
}

// WriteFloat32 encodes and writes a float32 value.
func (w *Writer) WriteFloat32(val float32) {
	if w.err != nil {
		return
	}
	if math.IsNaN(float64(val)) || math.IsInf(float64(val), 0) {
		w.recordError(ErrInvalidFloat)
		return
	}
	w.WriteTag(TagF32)
	w.writeUint32LE(math.Float32bits(val))
}

// WriteFloat64 encodes and writes a float64 value.
func (w *Writer) WriteFloat64(val float64) {
	if w.err != nil {
		return
	}
	if math.IsNaN(val) || math.IsInf(val, 0) {
		w.recordError(ErrInvalidFloat)
		return
	}
	w.WriteTag(TagF64)
	w.writeUint64LE(math.Float64bits(val))
}

// WriteString encodes and writes a string value.
func (w *Writer) WriteString(val string) {
	if w.err != nil {
		return
	}
	if !utf8.ValidString(val) {
		w.recordError(ErrInvalidUTF8)
		return
	}
	if len(val) > MaxPayloadLen {
		w.recordError(ErrTooLarge)
		return
	}
	strBytes := []byte(val)
	w.WriteTag(TagString)
	w.writeUint32LE(uint32(len(strBytes))) // Length prefix
	if len(strBytes) > 0 {
		_, err := w.w.Write(strBytes)
		if err == nil {
			w.bytesWritten += len(strBytes)
		}
		w.recordError(err)
	}
}

// WriteBytes encodes and writes a byte slice.
func (w *Writer) WriteBytes(val []byte) {
	if w.err != nil {
		return
	}
	if len(val) > MaxPayloadLen {
		w.recordError(ErrTooLarge)
		return
	}
	w.WriteTag(TagBytes)
	w.writeUint32LE(uint32(len(val))) // Length prefix
	if len(val) > 0 {
		_, err := w.w.Write(val)
		if err == nil {
			w.bytesWritten += len(val)
		}
		w.recordError(err)
	}
}

// WriteNilOption writes a TagOptionNone.
func (w *Writer) WriteNilOption() {
	if w.err != nil {
		return
	}
	w.WriteTag(TagOptionNone)
}

// WriteSomeTag writes TagOptionSome. The caller is responsible for writing the payload next.
func (w *Writer) WriteSomeTag() {
	if w.err != nil {
		return
	}
	w.WriteTag(TagOptionSome)
}

// WriteU128Bytes writes TagU128 followed by 16 bytes.
func (w *Writer) WriteU128Bytes(val [16]byte) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagU128)
	_, err := w.w.Write(val[:])
	w.recordError(err)
}

// WriteI128Bytes writes TagI128 followed by 16 bytes.
func (w *Writer) WriteI128Bytes(val [16]byte) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagI128)
	_, err := w.w.Write(val[:])
	w.recordError(err)
}

// WriteU256Bytes writes TagU256 followed by 32 bytes.
func (w *Writer) WriteU256Bytes(val [32]byte) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagU256)
	_, err := w.w.Write(val[:])
	w.recordError(err)
}

// WriteI256Bytes writes TagI256 followed by 32 bytes.
func (w *Writer) WriteI256Bytes(val [32]byte) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagI256)
	_, err := w.w.Write(val[:])
	w.recordError(err)
}

// WriteListHeader writes the TagList and the count of items.
// The caller is then responsible for writing each item.
func (w *Writer) WriteListHeader(count int) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagList)
	w.writeUint32LE(uint32(count))
}

// WriteArrayHeader writes the TagArray and the count of items.
// The caller is then responsible for writing each item.
func (w *Writer) WriteArrayHeader(count int) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagArray)
	w.writeUint32LE(uint32(count))
}

// WriteStructHeader writes the TagStruct and the field count.
// The caller is then responsible for writing each field (nameLen, name, value).
func (w *Writer) WriteStructHeader(fieldCount int) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagStruct)
	w.writeUint32LE(uint32(fieldCount))
}

// WriteFieldName encodes and writes a field name for a struct.
// Field names are length-prefixed (u8) and then UTF-8 bytes.
func (w *Writer) WriteFieldName(name string) {
	if w.err != nil {
		return
	}
	nameBytes := []byte(name)
	if len(nameBytes) > 255 {
		w.recordError(fmt.Errorf("bsatn: field name '%s' too long (%d bytes), max 255", name, len(nameBytes)))
		return
	}
	if !utf8.ValidString(name) {
		w.recordError(fmt.Errorf("bsatn: field name '%s' is not valid UTF-8", name))
		return
	}
	lenByte := byte(len(nameBytes))
	_, err := w.w.Write([]byte{lenByte})
	if err == nil {
		w.bytesWritten++
	}
	if err != nil {
		w.recordError(err)
		return
	}
	if len(nameBytes) > 0 {
		_, err = w.w.Write(nameBytes)
		if err == nil {
			w.bytesWritten += len(nameBytes)
		}
		w.recordError(err)
	}
}

// WriteEnumHeader writes the TagEnum and the variant index.
// The caller is then responsible for writing the variant's payload (if any).
func (w *Writer) WriteEnumHeader(variantIndex uint32) {
	if w.err != nil {
		return
	}
	w.WriteTag(TagEnum)
	w.writeUint32LE(variantIndex)
}
