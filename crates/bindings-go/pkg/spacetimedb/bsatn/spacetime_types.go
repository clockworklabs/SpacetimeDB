package bsatn

import (
	"io"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb/types"
)

// SpacetimeDB Core Types BSATN Encoders/Decoders
// These provide BSATN serialization for SpacetimeDB's core types

// EncodeIdentity encodes an Identity value in BSATN format
func EncodeIdentity(w io.Writer, val types.Identity) error {
	// Identity is a 16-byte array, write directly
	_, err := w.Write(val.Bytes[:])
	if err != nil {
		return &EncodingError{Type: "Identity", Reason: "write failed", Err: err}
	}
	return nil
}

// DecodeIdentity decodes an Identity value from BSATN format
func DecodeIdentity(r io.Reader) (types.Identity, error) {
	var identity types.Identity
	_, err := io.ReadFull(r, identity.Bytes[:])
	if err != nil {
		return identity, &DecodingError{Type: "Identity", Reason: "read failed", Err: err}
	}
	return identity, nil
}

// EncodeTimestamp encodes a Timestamp value in BSATN format
func EncodeTimestamp(w io.Writer, val types.Timestamp) error {
	// Timestamp is stored as uint64 microseconds
	return EncodeU64(w, val.Microseconds)
}

// DecodeTimestamp decodes a Timestamp value from BSATN format
func DecodeTimestamp(r io.Reader) (types.Timestamp, error) {
	microseconds, err := DecodeU64(r)
	if err != nil {
		return types.Timestamp{}, &DecodingError{Type: "Timestamp", Reason: "failed to decode microseconds", Err: err}
	}
	return types.Timestamp{Microseconds: microseconds}, nil
}

// EncodeTimeDuration encodes a TimeDuration value in BSATN format
func EncodeTimeDuration(w io.Writer, val types.TimeDuration) error {
	// TimeDuration is stored as uint64 microseconds
	return EncodeU64(w, val.Microseconds)
}

// DecodeTimeDuration decodes a TimeDuration value from BSATN format
func DecodeTimeDuration(r io.Reader) (types.TimeDuration, error) {
	microseconds, err := DecodeU64(r)
	if err != nil {
		return types.TimeDuration{}, &DecodingError{Type: "TimeDuration", Reason: "failed to decode microseconds", Err: err}
	}
	return types.TimeDuration{Microseconds: microseconds}, nil
}

// EncodeScheduleAt encodes a ScheduleAt value in BSATN format
func EncodeScheduleAt(w io.Writer, val types.ScheduleAt) error {
	// ScheduleAt is an enum-like type with either Time or Interval
	if val.Time != nil {
		// Tag = 0 for Time variant
		if err := EncodeU8(w, 0); err != nil {
			return &EncodingError{Type: "ScheduleAt", Reason: "failed to encode Time tag", Err: err}
		}
		return EncodeTimestamp(w, *val.Time)
	} else if val.Interval != nil {
		// Tag = 1 for Interval variant
		if err := EncodeU8(w, 1); err != nil {
			return &EncodingError{Type: "ScheduleAt", Reason: "failed to encode Interval tag", Err: err}
		}
		return EncodeTimeDuration(w, *val.Interval)
	} else {
		// Tag = 2 for None variant (shouldn't happen in valid usage)
		return EncodeU8(w, 2)
	}
}

// DecodeScheduleAt decodes a ScheduleAt value from BSATN format
func DecodeScheduleAt(r io.Reader) (types.ScheduleAt, error) {
	tag, err := DecodeU8(r)
	if err != nil {
		return types.ScheduleAt{}, &DecodingError{Type: "ScheduleAt", Reason: "failed to decode tag", Err: err}
	}

	switch tag {
	case 0: // Time variant
		timestamp, err := DecodeTimestamp(r)
		if err != nil {
			return types.ScheduleAt{}, &DecodingError{Type: "ScheduleAt", Reason: "failed to decode Time", Err: err}
		}
		return types.ScheduleAt{Time: &timestamp}, nil

	case 1: // Interval variant
		duration, err := DecodeTimeDuration(r)
		if err != nil {
			return types.ScheduleAt{}, &DecodingError{Type: "ScheduleAt", Reason: "failed to decode Interval", Err: err}
		}
		return types.ScheduleAt{Interval: &duration}, nil

	case 2: // None variant
		return types.ScheduleAt{}, nil

	default:
		return types.ScheduleAt{}, &DecodingError{Type: "ScheduleAt", Reason: "invalid tag"}
	}
}

// Size calculation functions for SpacetimeDB types

// SizeIdentity returns the size of an Identity when serialized (always 16)
func SizeIdentity() int {
	return 16
}

// SizeTimestamp returns the size of a Timestamp when serialized (always 8)
func SizeTimestamp() int {
	return 8
}

// SizeTimeDuration returns the size of a TimeDuration when serialized (always 8)
func SizeTimeDuration() int {
	return 8
}

// SizeScheduleAt returns the size of a ScheduleAt when serialized
func SizeScheduleAt(val types.ScheduleAt) int {
	// Tag (1 byte) + variant data
	if val.Time != nil {
		return 1 + SizeTimestamp()
	} else if val.Interval != nil {
		return 1 + SizeTimeDuration()
	} else {
		return 1 // Just the tag for None variant
	}
}

// Typed Codecs for SpacetimeDB types

// IdentityCodec provides BSATN encoding/decoding for Identity
type IdentityCodec struct {
	Value types.Identity
}

func (ic *IdentityCodec) Encode(w io.Writer) error {
	return EncodeIdentity(w, ic.Value)
}

func (ic *IdentityCodec) Decode(r io.Reader) error {
	val, err := DecodeIdentity(r)
	if err != nil {
		return err
	}
	ic.Value = val
	return nil
}

func (ic *IdentityCodec) BsatnSize() int {
	return SizeIdentity()
}

// TimestampCodec provides BSATN encoding/decoding for Timestamp
type TimestampCodec struct {
	Value types.Timestamp
}

func (tc *TimestampCodec) Encode(w io.Writer) error {
	return EncodeTimestamp(w, tc.Value)
}

func (tc *TimestampCodec) Decode(r io.Reader) error {
	val, err := DecodeTimestamp(r)
	if err != nil {
		return err
	}
	tc.Value = val
	return nil
}

func (tc *TimestampCodec) BsatnSize() int {
	return SizeTimestamp()
}

// TimeDurationCodec provides BSATN encoding/decoding for TimeDuration
type TimeDurationCodec struct {
	Value types.TimeDuration
}

func (tdc *TimeDurationCodec) Encode(w io.Writer) error {
	return EncodeTimeDuration(w, tdc.Value)
}

func (tdc *TimeDurationCodec) Decode(r io.Reader) error {
	val, err := DecodeTimeDuration(r)
	if err != nil {
		return err
	}
	tdc.Value = val
	return nil
}

func (tdc *TimeDurationCodec) BsatnSize() int {
	return SizeTimeDuration()
}

// ScheduleAtCodec provides BSATN encoding/decoding for ScheduleAt
type ScheduleAtCodec struct {
	Value types.ScheduleAt
}

func (sac *ScheduleAtCodec) Encode(w io.Writer) error {
	return EncodeScheduleAt(w, sac.Value)
}

func (sac *ScheduleAtCodec) Decode(r io.Reader) error {
	val, err := DecodeScheduleAt(r)
	if err != nil {
		return err
	}
	sac.Value = val
	return nil
}

func (sac *ScheduleAtCodec) BsatnSize() int {
	return SizeScheduleAt(sac.Value)
}

// Convenience constructors for codecs

// NewIdentityCodec creates a new IdentityCodec with the given value
func NewIdentityCodec(val types.Identity) *IdentityCodec {
	return &IdentityCodec{Value: val}
}

// NewTimestampCodec creates a new TimestampCodec with the given value
func NewTimestampCodec(val types.Timestamp) *TimestampCodec {
	return &TimestampCodec{Value: val}
}

// NewTimeDurationCodec creates a new TimeDurationCodec with the given value
func NewTimeDurationCodec(val types.TimeDuration) *TimeDurationCodec {
	return &TimeDurationCodec{Value: val}
}

// NewScheduleAtCodec creates a new ScheduleAtCodec with the given value
func NewScheduleAtCodec(val types.ScheduleAt) *ScheduleAtCodec {
	return &ScheduleAtCodec{Value: val}
}

// Utility functions for SpacetimeDB types

// IdentityToBytes serializes an Identity to bytes
func IdentityToBytes(val types.Identity) ([]byte, error) {
	return ToBytes(func(w io.Writer) error {
		return EncodeIdentity(w, val)
	})
}

// IdentityFromBytes deserializes an Identity from bytes
func IdentityFromBytes(data []byte) (types.Identity, error) {
	var val types.Identity
	err := FromBytes(data, func(r io.Reader) error {
		var err error
		val, err = DecodeIdentity(r)
		return err
	})
	return val, err
}

// TimestampToBytes serializes a Timestamp to bytes
func TimestampToBytes(val types.Timestamp) ([]byte, error) {
	return ToBytes(func(w io.Writer) error {
		return EncodeTimestamp(w, val)
	})
}

// TimestampFromBytes deserializes a Timestamp from bytes
func TimestampFromBytes(data []byte) (types.Timestamp, error) {
	var val types.Timestamp
	err := FromBytes(data, func(r io.Reader) error {
		var err error
		val, err = DecodeTimestamp(r)
		return err
	})
	return val, err
}

// TimeDurationToBytes serializes a TimeDuration to bytes
func TimeDurationToBytes(val types.TimeDuration) ([]byte, error) {
	return ToBytes(func(w io.Writer) error {
		return EncodeTimeDuration(w, val)
	})
}

// TimeDurationFromBytes deserializes a TimeDuration from bytes
func TimeDurationFromBytes(data []byte) (types.TimeDuration, error) {
	var val types.TimeDuration
	err := FromBytes(data, func(r io.Reader) error {
		var err error
		val, err = DecodeTimeDuration(r)
		return err
	})
	return val, err
}

// ScheduleAtToBytes serializes a ScheduleAt to bytes
func ScheduleAtToBytes(val types.ScheduleAt) ([]byte, error) {
	return ToBytes(func(w io.Writer) error {
		return EncodeScheduleAt(w, val)
	})
}

// ScheduleAtFromBytes deserializes a ScheduleAt from bytes
func ScheduleAtFromBytes(data []byte) (types.ScheduleAt, error) {
	var val types.ScheduleAt
	err := FromBytes(data, func(r io.Reader) error {
		var err error
		val, err = DecodeScheduleAt(r)
		return err
	})
	return val, err
}
