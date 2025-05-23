package types

import (
	"encoding/json"
	"fmt"
	"time"
)

// Core SpacetimeDB Types
// These types represent the fundamental types used by SpacetimeDB across all games

// Identity represents a player's unique identity in SpacetimeDB
// This matches the SpacetimeDB Identity type used in Rust and C#
type Identity struct {
	Bytes [16]byte `json:"bytes" bsatn:"0"`
}

// Timestamp represents a point in time
// This matches the SpacetimeDB Timestamp type used in Rust and C#
type Timestamp struct {
	Microseconds uint64 `json:"microseconds" bsatn:"0"`
}

// TimeDuration represents a duration of time
// This matches the SpacetimeDB TimeDuration type used in Rust and C#
type TimeDuration struct {
	Microseconds uint64 `json:"microseconds" bsatn:"0"`
}

// ScheduleAt represents when a scheduled reducer should be executed
// This matches the SpacetimeDB ScheduleAt type used in Rust and C#
type ScheduleAt struct {
	Time     *Timestamp    `json:"time,omitempty" bsatn:"0"`
	Interval *TimeDuration `json:"interval,omitempty" bsatn:"1"`
}

// Identity Methods

// NewIdentity creates a new Identity from bytes
func NewIdentity(bytes [16]byte) Identity {
	return Identity{Bytes: bytes}
}

// String returns a string representation of the Identity
func (i Identity) String() string {
	return fmt.Sprintf("Identity(%x)", i.Bytes)
}

// IsZero returns true if the identity is all zeros
func (i Identity) IsZero() bool {
	for _, b := range i.Bytes {
		if b != 0 {
			return false
		}
	}
	return true
}

// MarshalJSON implements JSON encoding for Identity
func (i Identity) MarshalJSON() ([]byte, error) {
	return json.Marshal(fmt.Sprintf("%x", i.Bytes))
}

// UnmarshalJSON implements JSON decoding for Identity
func (i *Identity) UnmarshalJSON(data []byte) error {
	var hexStr string
	if err := json.Unmarshal(data, &hexStr); err != nil {
		return err
	}

	if len(hexStr) != 32 {
		return fmt.Errorf("invalid identity hex string length: expected 32, got %d", len(hexStr))
	}

	for idx := 0; idx < 16; idx++ {
		var b byte
		_, err := fmt.Sscanf(hexStr[idx*2:idx*2+2], "%02x", &b)
		if err != nil {
			return fmt.Errorf("invalid hex character at position %d: %w", idx*2, err)
		}
		i.Bytes[idx] = b
	}

	return nil
}

// Timestamp Methods

// NewTimestamp creates a new Timestamp from microseconds
func NewTimestamp(microseconds uint64) Timestamp {
	return Timestamp{Microseconds: microseconds}
}

// NewTimestampFromTime creates a new Timestamp from a Go time.Time
func NewTimestampFromTime(t time.Time) Timestamp {
	return Timestamp{Microseconds: uint64(t.UnixNano() / 1000)}
}

// ToTime converts a Timestamp to a Go time.Time
func (t Timestamp) ToTime() time.Time {
	return time.Unix(0, int64(t.Microseconds)*1000)
}

// String returns a string representation of the Timestamp
func (t Timestamp) String() string {
	return t.ToTime().Format(time.RFC3339Nano)
}

// Add adds a duration to the timestamp
func (t Timestamp) Add(duration TimeDuration) Timestamp {
	return Timestamp{Microseconds: t.Microseconds + duration.Microseconds}
}

// Sub subtracts another timestamp from this one, returning the duration
func (t Timestamp) Sub(other Timestamp) TimeDuration {
	if t.Microseconds >= other.Microseconds {
		return TimeDuration{Microseconds: t.Microseconds - other.Microseconds}
	}
	return TimeDuration{Microseconds: 0}
}

// Before returns true if this timestamp is before the other
func (t Timestamp) Before(other Timestamp) bool {
	return t.Microseconds < other.Microseconds
}

// After returns true if this timestamp is after the other
func (t Timestamp) After(other Timestamp) bool {
	return t.Microseconds > other.Microseconds
}

// Equal returns true if this timestamp equals the other
func (t Timestamp) Equal(other Timestamp) bool {
	return t.Microseconds == other.Microseconds
}

// TimeDuration Methods

// NewTimeDuration creates a new TimeDuration from microseconds
func NewTimeDuration(microseconds uint64) TimeDuration {
	return TimeDuration{Microseconds: microseconds}
}

// NewTimeDurationFromDuration creates a new TimeDuration from a Go time.Duration
func NewTimeDurationFromDuration(d time.Duration) TimeDuration {
	return TimeDuration{Microseconds: uint64(d.Nanoseconds() / 1000)}
}

// ToDuration converts a TimeDuration to a Go time.Duration
func (d TimeDuration) ToDuration() time.Duration {
	return time.Duration(d.Microseconds) * time.Microsecond
}

// String returns a string representation of the TimeDuration
func (d TimeDuration) String() string {
	return d.ToDuration().String()
}

// Seconds returns the duration in seconds as a float64
func (d TimeDuration) Seconds() float64 {
	return float64(d.Microseconds) / 1000000.0
}

// Milliseconds returns the duration in milliseconds
func (d TimeDuration) Milliseconds() uint64 {
	return d.Microseconds / 1000
}

// Add adds another duration to this one
func (d TimeDuration) Add(other TimeDuration) TimeDuration {
	return TimeDuration{Microseconds: d.Microseconds + other.Microseconds}
}

// Sub subtracts another duration from this one
func (d TimeDuration) Sub(other TimeDuration) TimeDuration {
	if d.Microseconds >= other.Microseconds {
		return TimeDuration{Microseconds: d.Microseconds - other.Microseconds}
	}
	return TimeDuration{Microseconds: 0}
}

// ScheduleAt Methods

// NewScheduleAtTime creates a ScheduleAt for a specific time
func NewScheduleAtTime(timestamp Timestamp) ScheduleAt {
	return ScheduleAt{Time: &timestamp, Interval: nil}
}

// NewScheduleAtInterval creates a ScheduleAt for a repeating interval
func NewScheduleAtInterval(interval TimeDuration) ScheduleAt {
	return ScheduleAt{Time: nil, Interval: &interval}
}

// IsTime returns true if this is a time-based schedule
func (s ScheduleAt) IsTime() bool {
	return s.Time != nil
}

// IsInterval returns true if this is an interval-based schedule
func (s ScheduleAt) IsInterval() bool {
	return s.Interval != nil
}

// GetTime returns the time if this is a time-based schedule
func (s ScheduleAt) GetTime() *Timestamp {
	return s.Time
}

// GetInterval returns the interval if this is an interval-based schedule
func (s ScheduleAt) GetInterval() *TimeDuration {
	return s.Interval
}

// String returns a string representation of ScheduleAt
func (s ScheduleAt) String() string {
	if s.IsTime() {
		return fmt.Sprintf("ScheduleAt(Time: %s)", s.Time.String())
	} else if s.IsInterval() {
		return fmt.Sprintf("ScheduleAt(Interval: %s)", s.Interval.String())
	}
	return "ScheduleAt(None)"
}

// Validation Methods

// Validate validates a Timestamp
func (t Timestamp) Validate() error {
	// Timestamps should be reasonable (not in far future)
	maxTimestamp := uint64(time.Date(2100, 1, 1, 0, 0, 0, 0, time.UTC).UnixNano() / 1000)
	if t.Microseconds > maxTimestamp {
		return fmt.Errorf("timestamp too far in future: %d", t.Microseconds)
	}
	return nil
}

// Validate validates a TimeDuration
func (d TimeDuration) Validate() error {
	// Durations should be reasonable (less than 1000 years)
	// Calculate in microseconds: 1000 years * 365 days * 24 hours * 3600 seconds * 1000000 microseconds
	maxDuration := uint64(1000 * 365 * 24 * 3600 * 1000000)
	if d.Microseconds > maxDuration {
		return fmt.Errorf("duration too long: %d microseconds", d.Microseconds)
	}
	return nil
}

// Validate validates a ScheduleAt
func (s ScheduleAt) Validate() error {
	if s.Time != nil && s.Interval != nil {
		return fmt.Errorf("ScheduleAt cannot have both time and interval")
	}
	if s.Time == nil && s.Interval == nil {
		return fmt.Errorf("ScheduleAt must have either time or interval")
	}
	if s.Time != nil {
		return s.Time.Validate()
	}
	if s.Interval != nil {
		return s.Interval.Validate()
	}
	return nil
}
