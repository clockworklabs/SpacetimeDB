package types

import (
	"encoding/json"
	"strings"
	"testing"
	"time"
)

func TestIdentity(t *testing.T) {
	t.Run("NewIdentity", func(t *testing.T) {
		bytes := [16]byte{0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10}
		identity := NewIdentity(bytes)
		if identity.Bytes != bytes {
			t.Errorf("NewIdentity() = %v, want %v", identity.Bytes, bytes)
		}
	})

	t.Run("IsZero", func(t *testing.T) {
		zero := Identity{}
		if !zero.IsZero() {
			t.Error("IsZero() = false for zero identity, want true")
		}

		nonZero := NewIdentity([16]byte{0x01})
		if nonZero.IsZero() {
			t.Error("IsZero() = true for non-zero identity, want false")
		}
	})

	t.Run("String", func(t *testing.T) {
		identity := NewIdentity([16]byte{0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10})
		expected := "Identity(0102030405060708090a0b0c0d0e0f10)"
		if identity.String() != expected {
			t.Errorf("String() = %q, want %q", identity.String(), expected)
		}
	})

	t.Run("JSON", func(t *testing.T) {
		identity := NewIdentity([16]byte{0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10})

		// Test marshaling
		data, err := json.Marshal(identity)
		if err != nil {
			t.Fatalf("json.Marshal() error = %v", err)
		}

		// Test unmarshaling
		var decoded Identity
		err = json.Unmarshal(data, &decoded)
		if err != nil {
			t.Fatalf("json.Unmarshal() error = %v", err)
		}

		if decoded.Bytes != identity.Bytes {
			t.Errorf("JSON roundtrip failed: got %v, want %v", decoded.Bytes, identity.Bytes)
		}
	})

	t.Run("JSON Error Cases", func(t *testing.T) {
		// Test invalid hex string length (too short)
		var identity Identity
		err := json.Unmarshal([]byte(`"0102030405060708090a0b0c0d0e0f"`), &identity)
		if err == nil {
			t.Error("Expected error for hex string too short, got nil")
		}
		if err != nil && !strings.Contains(err.Error(), "invalid identity hex string length") {
			t.Errorf("Expected length error, got: %v", err)
		}

		// Test invalid hex string length (too long)
		err = json.Unmarshal([]byte(`"0102030405060708090a0b0c0d0e0f1011"`), &identity)
		if err == nil {
			t.Error("Expected error for hex string too long, got nil")
		}
		if err != nil && !strings.Contains(err.Error(), "invalid identity hex string length") {
			t.Errorf("Expected length error, got: %v", err)
		}

		// Test invalid hex characters
		err = json.Unmarshal([]byte(`"0102030405060708090a0b0c0d0eXX10"`), &identity)
		if err == nil {
			t.Error("Expected error for invalid hex character, got nil")
		}
		if err != nil && !strings.Contains(err.Error(), "invalid hex character") {
			t.Errorf("Expected hex character error, got: %v", err)
		}

		// Test non-hex characters
		err = json.Unmarshal([]byte(`"ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ"`), &identity)
		if err == nil {
			t.Error("Expected error for non-hex characters, got nil")
		}

		// Test invalid JSON (not a string)
		err = json.Unmarshal([]byte(`123`), &identity)
		if err == nil {
			t.Error("Expected error for non-string JSON, got nil")
		}

		// Test empty string
		err = json.Unmarshal([]byte(`""`), &identity)
		if err == nil {
			t.Error("Expected error for empty string, got nil")
		}
		if err != nil && !strings.Contains(err.Error(), "invalid identity hex string length") {
			t.Errorf("Expected length error for empty string, got: %v", err)
		}
	})
}

func TestTimestamp(t *testing.T) {
	t.Run("NewTimestamp", func(t *testing.T) {
		microseconds := uint64(1234567890)
		timestamp := NewTimestamp(microseconds)
		if timestamp.Microseconds != microseconds {
			t.Errorf("NewTimestamp() = %v, want %v", timestamp.Microseconds, microseconds)
		}
	})

	t.Run("NewTimestampFromTime", func(t *testing.T) {
		now := time.Now()
		timestamp := NewTimestampFromTime(now)
		expectedMicros := uint64(now.UnixNano() / 1000)
		if timestamp.Microseconds != expectedMicros {
			t.Errorf("NewTimestampFromTime() = %v, want %v", timestamp.Microseconds, expectedMicros)
		}
	})

	t.Run("ToTime", func(t *testing.T) {
		now := time.Now().Truncate(time.Microsecond)
		timestamp := NewTimestampFromTime(now)
		converted := timestamp.ToTime()
		if !converted.Equal(now) {
			t.Errorf("ToTime() = %v, want %v", converted, now)
		}
	})

	t.Run("Add", func(t *testing.T) {
		timestamp := NewTimestamp(1000000)
		duration := NewTimeDuration(500000)
		result := timestamp.Add(duration)
		expected := uint64(1500000)
		if result.Microseconds != expected {
			t.Errorf("Add() = %v, want %v", result.Microseconds, expected)
		}
	})

	t.Run("Sub", func(t *testing.T) {
		timestamp1 := NewTimestamp(1500000)
		timestamp2 := NewTimestamp(1000000)
		result := timestamp1.Sub(timestamp2)
		expected := uint64(500000)
		if result.Microseconds != expected {
			t.Errorf("Sub() = %v, want %v", result.Microseconds, expected)
		}
	})

	t.Run("Before/After/Equal", func(t *testing.T) {
		timestamp1 := NewTimestamp(1000000)
		timestamp2 := NewTimestamp(2000000)
		timestamp3 := NewTimestamp(1000000)

		if !timestamp1.Before(timestamp2) {
			t.Error("Before() should return true for earlier timestamp")
		}

		if !timestamp2.After(timestamp1) {
			t.Error("After() should return true for later timestamp")
		}

		if !timestamp1.Equal(timestamp3) {
			t.Error("Equal() should return true for same timestamp")
		}
	})

	t.Run("Validate", func(t *testing.T) {
		// Valid timestamp
		validTimestamp := NewTimestamp(uint64(time.Now().UnixNano() / 1000))
		if err := validTimestamp.Validate(); err != nil {
			t.Errorf("Validate() error = %v for valid timestamp", err)
		}

		// Invalid timestamp (too far in future)
		invalidTimestamp := NewTimestamp(uint64(time.Date(2200, 1, 1, 0, 0, 0, 0, time.UTC).UnixNano() / 1000))
		if err := invalidTimestamp.Validate(); err == nil {
			t.Error("Validate() should return error for timestamp too far in future")
		}
	})
}

func TestTimeDuration(t *testing.T) {
	t.Run("NewTimeDuration", func(t *testing.T) {
		microseconds := uint64(1234567)
		duration := NewTimeDuration(microseconds)
		if duration.Microseconds != microseconds {
			t.Errorf("NewTimeDuration() = %v, want %v", duration.Microseconds, microseconds)
		}
	})

	t.Run("NewTimeDurationFromDuration", func(t *testing.T) {
		goDuration := 5 * time.Second
		duration := NewTimeDurationFromDuration(goDuration)
		expected := uint64(goDuration.Nanoseconds() / 1000)
		if duration.Microseconds != expected {
			t.Errorf("NewTimeDurationFromDuration() = %v, want %v", duration.Microseconds, expected)
		}
	})

	t.Run("ToDuration", func(t *testing.T) {
		microseconds := uint64(5000000) // 5 seconds
		duration := NewTimeDuration(microseconds)
		goDuration := duration.ToDuration()
		expected := 5 * time.Second
		if goDuration != expected {
			t.Errorf("ToDuration() = %v, want %v", goDuration, expected)
		}
	})

	t.Run("Seconds", func(t *testing.T) {
		duration := NewTimeDuration(5000000) // 5 seconds
		seconds := duration.Seconds()
		expected := 5.0
		if seconds != expected {
			t.Errorf("Seconds() = %v, want %v", seconds, expected)
		}
	})

	t.Run("Milliseconds", func(t *testing.T) {
		duration := NewTimeDuration(5000000) // 5 seconds
		milliseconds := duration.Milliseconds()
		expected := uint64(5000)
		if milliseconds != expected {
			t.Errorf("Milliseconds() = %v, want %v", milliseconds, expected)
		}
	})

	t.Run("Add", func(t *testing.T) {
		duration1 := NewTimeDuration(1000000)
		duration2 := NewTimeDuration(500000)
		result := duration1.Add(duration2)
		expected := uint64(1500000)
		if result.Microseconds != expected {
			t.Errorf("Add() = %v, want %v", result.Microseconds, expected)
		}
	})

	t.Run("Sub", func(t *testing.T) {
		duration1 := NewTimeDuration(1500000)
		duration2 := NewTimeDuration(500000)
		result := duration1.Sub(duration2)
		expected := uint64(1000000)
		if result.Microseconds != expected {
			t.Errorf("Sub() = %v, want %v", result.Microseconds, expected)
		}

		// Test underflow protection
		duration3 := NewTimeDuration(300000)
		result2 := duration3.Sub(duration2)
		expected2 := uint64(0)
		if result2.Microseconds != expected2 {
			t.Errorf("Sub() underflow = %v, want %v", result2.Microseconds, expected2)
		}
	})

	t.Run("Validate", func(t *testing.T) {
		// Valid duration
		validDuration := NewTimeDuration(1000000)
		if err := validDuration.Validate(); err != nil {
			t.Errorf("Validate() error = %v for valid duration", err)
		}

		// Invalid duration (too long)
		invalidDuration := NewTimeDuration(uint64(2000 * 365 * 24 * 3600 * 1000000)) // 2000 years
		if err := invalidDuration.Validate(); err == nil {
			t.Error("Validate() should return error for duration too long")
		}
	})
}

func TestScheduleAt(t *testing.T) {
	t.Run("NewScheduleAtTime", func(t *testing.T) {
		timestamp := NewTimestamp(1234567890)
		schedule := NewScheduleAtTime(timestamp)
		if !schedule.IsTime() {
			t.Error("IsTime() should return true for time-based schedule")
		}
		if schedule.IsInterval() {
			t.Error("IsInterval() should return false for time-based schedule")
		}
		if schedule.GetTime().Microseconds != timestamp.Microseconds {
			t.Errorf("GetTime() = %v, want %v", schedule.GetTime().Microseconds, timestamp.Microseconds)
		}
	})

	t.Run("NewScheduleAtInterval", func(t *testing.T) {
		interval := NewTimeDuration(1000000)
		schedule := NewScheduleAtInterval(interval)
		if schedule.IsTime() {
			t.Error("IsTime() should return false for interval-based schedule")
		}
		if !schedule.IsInterval() {
			t.Error("IsInterval() should return true for interval-based schedule")
		}
		if schedule.GetInterval().Microseconds != interval.Microseconds {
			t.Errorf("GetInterval() = %v, want %v", schedule.GetInterval().Microseconds, interval.Microseconds)
		}
	})

	t.Run("Validate", func(t *testing.T) {
		// Valid time-based schedule
		validTimeSchedule := NewScheduleAtTime(NewTimestamp(1234567890))
		if err := validTimeSchedule.Validate(); err != nil {
			t.Errorf("Validate() error = %v for valid time schedule", err)
		}

		// Valid interval-based schedule
		validIntervalSchedule := NewScheduleAtInterval(NewTimeDuration(1000000))
		if err := validIntervalSchedule.Validate(); err != nil {
			t.Errorf("Validate() error = %v for valid interval schedule", err)
		}

		// Invalid schedule (both time and interval)
		invalidSchedule := ScheduleAt{
			Time:     &Timestamp{Microseconds: 1234567890},
			Interval: &TimeDuration{Microseconds: 1000000},
		}
		if err := invalidSchedule.Validate(); err == nil {
			t.Error("Validate() should return error for schedule with both time and interval")
		}

		// Invalid schedule (neither time nor interval)
		emptySchedule := ScheduleAt{}
		if err := emptySchedule.Validate(); err == nil {
			t.Error("Validate() should return error for schedule with neither time nor interval")
		}
	})

	t.Run("String", func(t *testing.T) {
		// Time-based schedule
		timeSchedule := NewScheduleAtTime(NewTimestamp(1234567890))
		timeStr := timeSchedule.String()
		if timeStr == "" {
			t.Error("String() should not be empty for time-based schedule")
		}

		// Interval-based schedule
		intervalSchedule := NewScheduleAtInterval(NewTimeDuration(1000000))
		intervalStr := intervalSchedule.String()
		if intervalStr == "" {
			t.Error("String() should not be empty for interval-based schedule")
		}

		// Empty schedule
		emptySchedule := ScheduleAt{}
		emptyStr := emptySchedule.String()
		expected := "ScheduleAt(None)"
		if emptyStr != expected {
			t.Errorf("String() = %q, want %q for empty schedule", emptyStr, expected)
		}
	})
}

// Benchmark tests
func BenchmarkIdentityJSON(b *testing.B) {
	identity := NewIdentity([16]byte{0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10})
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		data, _ := json.Marshal(identity)
		var decoded Identity
		json.Unmarshal(data, &decoded)
	}
}

func BenchmarkTimestampOperations(b *testing.B) {
	timestamp1 := NewTimestamp(1000000)
	timestamp2 := NewTimestamp(500000)
	duration := NewTimeDuration(250000)
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		timestamp1.Add(duration)
		timestamp1.Sub(timestamp2)
		timestamp1.Before(timestamp2)
	}
}

func BenchmarkTimeDurationOperations(b *testing.B) {
	duration1 := NewTimeDuration(1000000)
	duration2 := NewTimeDuration(500000)
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		duration1.Add(duration2)
		duration1.Sub(duration2)
		duration1.Seconds()
	}
}
