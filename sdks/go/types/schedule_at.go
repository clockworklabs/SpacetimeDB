package types

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
)

// ScheduleAt is a sum type representing either a recurring interval or a specific time.
//   - Tag 0: Interval (TimeDuration)
//   - Tag 1: Time (Timestamp)
type ScheduleAt interface {
	bsatn.Serializable
	isScheduleAt()
}

// ScheduleAtInterval is the Interval variant of ScheduleAt (tag 0).
type ScheduleAtInterval struct {
	Value TimeDuration
}

func (ScheduleAtInterval) isScheduleAt() {}

func (s ScheduleAtInterval) WriteBsatn(w bsatn.Writer) {
	w.PutSumTag(0)
	s.Value.WriteBsatn(w)
}

// ScheduleAtTime is the Time variant of ScheduleAt (tag 1).
type ScheduleAtTime struct {
	Value Timestamp
}

func (ScheduleAtTime) isScheduleAt() {}

func (s ScheduleAtTime) WriteBsatn(w bsatn.Writer) {
	w.PutSumTag(1)
	s.Value.WriteBsatn(w)
}

// ReadScheduleAt reads a ScheduleAt from a BSATN reader.
func ReadScheduleAt(r bsatn.Reader) (ScheduleAt, error) {
	tag, err := r.GetSumTag()
	if err != nil {
		return nil, err
	}
	switch tag {
	case 0:
		v, err := ReadTimeDuration(r)
		if err != nil {
			return nil, err
		}
		return ScheduleAtInterval{Value: v}, nil
	case 1:
		v, err := ReadTimestamp(r)
		if err != nil {
			return nil, err
		}
		return ScheduleAtTime{Value: v}, nil
	default:
		return nil, &bsatn.ErrInvalidTag{Tag: tag, SumName: "ScheduleAt"}
	}
}
