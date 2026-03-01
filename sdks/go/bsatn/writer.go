package bsatn

import (
	"encoding/binary"
	"math"
)

// writer is the private BSATN writer implementation backed by a []byte buffer.
type writer struct {
	buf []byte
}

// NewWriter creates a new BSATN writer with an optional initial capacity.
func NewWriter(capacity int) Writer {
	return &writer{buf: make([]byte, 0, capacity)}
}

func (w *writer) PutBool(v bool) {
	if v {
		w.buf = append(w.buf, 0x01)
	} else {
		w.buf = append(w.buf, 0x00)
	}
}

func (w *writer) PutU8(v uint8) {
	w.buf = append(w.buf, v)
}

func (w *writer) PutU16(v uint16) {
	w.buf = binary.LittleEndian.AppendUint16(w.buf, v)
}

func (w *writer) PutU32(v uint32) {
	w.buf = binary.LittleEndian.AppendUint32(w.buf, v)
}

func (w *writer) PutU64(v uint64) {
	w.buf = binary.LittleEndian.AppendUint64(w.buf, v)
}

func (w *writer) PutI8(v int8) {
	w.buf = append(w.buf, uint8(v))
}

func (w *writer) PutI16(v int16) {
	w.buf = binary.LittleEndian.AppendUint16(w.buf, uint16(v))
}

func (w *writer) PutI32(v int32) {
	w.buf = binary.LittleEndian.AppendUint32(w.buf, uint32(v))
}

func (w *writer) PutI64(v int64) {
	w.buf = binary.LittleEndian.AppendUint64(w.buf, uint64(v))
}

func (w *writer) PutF32(v float32) {
	w.buf = binary.LittleEndian.AppendUint32(w.buf, math.Float32bits(v))
}

func (w *writer) PutF64(v float64) {
	w.buf = binary.LittleEndian.AppendUint64(w.buf, math.Float64bits(v))
}

func (w *writer) PutString(v string) {
	w.PutU32(uint32(len(v)))
	w.buf = append(w.buf, v...)
}

func (w *writer) PutBytes(v []byte) {
	w.buf = append(w.buf, v...)
}

func (w *writer) PutArrayLen(n uint32) {
	w.PutU32(n)
}

func (w *writer) PutMapLen(n uint32) {
	w.PutU32(n)
}

func (w *writer) PutSumTag(tag uint8) {
	w.PutU8(tag)
}

func (w *writer) Bytes() []byte {
	return w.buf
}

func (w *writer) Reset() {
	w.buf = w.buf[:0]
}

func (w *writer) Len() int {
	return len(w.buf)
}
