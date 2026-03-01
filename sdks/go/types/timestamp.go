package types

import (
	"time"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
)

// Timestamp wraps an i64 representing microseconds since the Unix epoch.
type Timestamp interface {
	bsatn.Serializable
	Microseconds() int64
	Time() time.Time
	String() string
}

// NewTimestamp creates a Timestamp from microseconds since Unix epoch.
func NewTimestamp(microseconds int64) Timestamp {
	return &timestamp{micros: microseconds}
}

// ReadTimestamp reads a Timestamp from a BSATN reader (i64).
func ReadTimestamp(r bsatn.Reader) (Timestamp, error) {
	v, err := r.GetI64()
	if err != nil {
		return nil, err
	}
	return &timestamp{micros: v}, nil
}

type timestamp struct {
	micros int64
}

func (t *timestamp) WriteBsatn(w bsatn.Writer) {
	w.PutI64(t.micros)
}

func (t *timestamp) Microseconds() int64 { return t.micros }

func (t *timestamp) Time() time.Time {
	return time.UnixMicro(t.micros)
}

func (t *timestamp) String() string {
	return t.Time().UTC().Format("2006-01-02T15:04:05.000000-07:00")
}
