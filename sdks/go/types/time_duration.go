package types

import (
	"fmt"
	"time"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
)

// TimeDuration wraps an i64 representing a duration in microseconds.
type TimeDuration interface {
	bsatn.Serializable
	Microseconds() int64
	Duration() time.Duration
	String() string
}

// NewTimeDuration creates a TimeDuration from microseconds.
func NewTimeDuration(microseconds int64) TimeDuration {
	return &timeDuration{micros: microseconds}
}

// ReadTimeDuration reads a TimeDuration from a BSATN reader (i64).
func ReadTimeDuration(r bsatn.Reader) (TimeDuration, error) {
	v, err := r.GetI64()
	if err != nil {
		return nil, err
	}
	return &timeDuration{micros: v}, nil
}

type timeDuration struct {
	micros int64
}

func (d *timeDuration) WriteBsatn(w bsatn.Writer) {
	w.PutI64(d.micros)
}

func (d *timeDuration) Microseconds() int64 { return d.micros }

func (d *timeDuration) Duration() time.Duration {
	// time.Duration is in nanoseconds, so multiply microseconds by 1000.
	return time.Duration(d.micros) * time.Microsecond
}

func (d *timeDuration) String() string {
	micros := d.micros
	sign := ""
	if micros < 0 {
		sign = "-"
		micros = -micros
	}
	secs := micros / 1_000_000
	remainder := micros % 1_000_000
	return fmt.Sprintf("%s%d.%06d", sign, secs, remainder)
}
