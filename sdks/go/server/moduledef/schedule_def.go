package moduledef

import "github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"

// ScheduleDef defines a schedule in the module.
type ScheduleDef interface {
	bsatn.Serializable
}

// NewScheduleDef creates a ScheduleDef.
// sourceName is optional (nil for auto-generated).
// tableName is the schedule table name.
// scheduleAtCol is the column index of the scheduled_at field.
// functionName is the reducer or procedure to call.
func NewScheduleDef(sourceName *string, tableName string, scheduleAtCol uint16, functionName string) ScheduleDef {
	return &scheduleDef{
		sourceName:    sourceName,
		tableName:     tableName,
		scheduleAtCol: scheduleAtCol,
		functionName:  functionName,
	}
}

type scheduleDef struct {
	sourceName    *string
	tableName     string
	scheduleAtCol uint16
	functionName  string
}

// WriteBsatn encodes the schedule definition as BSATN.
//
// Matches RawScheduleDefV10 product field order:
//
//	source_name: Option<String>
//	table_name: String
//	schedule_at_col: ColId (u16)
//	function_name: String
func (s *scheduleDef) WriteBsatn(w bsatn.Writer) {
	// source_name: Option<String>
	writeOptionString(w, s.sourceName)

	// table_name: String
	w.PutString(s.tableName)

	// schedule_at_col: ColId (u16)
	w.PutU16(s.scheduleAtCol)

	// function_name: String
	w.PutString(s.functionName)
}
