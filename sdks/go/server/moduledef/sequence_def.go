package moduledef

import (
	"encoding/binary"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
)

// SequenceDef defines a sequence for a table column.
type SequenceDef interface {
	bsatn.Serializable
}

// SequenceDefBuilder builds a SequenceDef.
type SequenceDefBuilder interface {
	WithStart(start int64) SequenceDefBuilder
	WithMinValue(min int64) SequenceDefBuilder
	WithMaxValue(max int64) SequenceDefBuilder
	WithIncrement(inc int64) SequenceDefBuilder
	Build() SequenceDef
}

// NewSequenceDefBuilder creates a SequenceDefBuilder.
// sourceName is optional (nil for auto-generated). column is the column ID.
func NewSequenceDefBuilder(sourceName *string, column uint16) SequenceDefBuilder {
	return &sequenceDef{
		sourceName: sourceName,
		column:     column,
		increment:  1,
	}
}

type sequenceDef struct {
	sourceName *string
	column     uint16
	start      *int64
	minValue   *int64
	maxValue   *int64
	increment  int64
}

func (s *sequenceDef) WithStart(start int64) SequenceDefBuilder {
	s.start = &start
	return s
}

func (s *sequenceDef) WithMinValue(min int64) SequenceDefBuilder {
	s.minValue = &min
	return s
}

func (s *sequenceDef) WithMaxValue(max int64) SequenceDefBuilder {
	s.maxValue = &max
	return s
}

func (s *sequenceDef) WithIncrement(inc int64) SequenceDefBuilder {
	s.increment = inc
	return s
}

func (s *sequenceDef) Build() SequenceDef {
	return s
}

// WriteBsatn encodes the sequence definition as BSATN.
//
// Matches RawSequenceDefV10 product field order:
//
//	source_name: Option<String>
//	column: ColId (u16)
//	start: Option<i128>
//	min_value: Option<i128>
//	max_value: Option<i128>
//	increment: i128
func (s *sequenceDef) WriteBsatn(w bsatn.Writer) {
	// source_name: Option<String>
	writeOptionString(w, s.sourceName)

	// column: ColId (u16)
	w.PutU16(s.column)

	// start: Option<i128>
	writeOptionI128(w, s.start)

	// min_value: Option<i128>
	writeOptionI128(w, s.minValue)

	// max_value: Option<i128>
	writeOptionI128(w, s.maxValue)

	// increment: i128 (16 bytes LE, sign-extended from int64)
	writeI128(w, s.increment)
}

// writeI128 writes an i128 value as 16 bytes LE, sign-extended from int64.
func writeI128(w bsatn.Writer, v int64) {
	var buf [16]byte
	binary.LittleEndian.PutUint64(buf[0:8], uint64(v))
	// Sign-extend: if negative, fill high 8 bytes with 0xFF
	var hi uint64
	if v < 0 {
		hi = ^uint64(0)
	}
	binary.LittleEndian.PutUint64(buf[8:16], hi)
	w.PutBytes(buf[:])
}

// writeOptionI128 writes an Option<i128> as BSATN.
func writeOptionI128(w bsatn.Writer, v *int64) {
	if v != nil {
		w.PutSumTag(0) // Some
		writeI128(w, *v)
	} else {
		w.PutSumTag(1) // None
	}
}
