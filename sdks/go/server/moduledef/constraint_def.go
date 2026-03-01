package moduledef

import "github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"

// ConstraintDef defines a constraint on a table.
type ConstraintDef interface {
	bsatn.Serializable
}

// NewUniqueConstraint creates a unique constraint definition.
// sourceName is optional (nil for auto-generated). columns are the column IDs.
func NewUniqueConstraint(sourceName *string, columns ...uint16) ConstraintDef {
	return &constraintDef{
		sourceName: sourceName,
		columns:    columns,
	}
}

type constraintDef struct {
	sourceName *string
	columns    []uint16
}

// WriteBsatn encodes the constraint definition as BSATN.
//
// Matches RawConstraintDefV10 product field order:
//
//	source_name: Option<String>
//	data: RawConstraintDataV10 (sum type, Unique=0)
//
// RawUniqueConstraintDataV9 product:
//
//	columns: ColList (array of u16)
func (c *constraintDef) WriteBsatn(w bsatn.Writer) {
	// source_name: Option<String>
	writeOptionString(w, c.sourceName)

	// data: RawConstraintDataV10 (sum type)
	// Only variant: Unique = tag 0
	w.PutSumTag(0)

	// RawUniqueConstraintDataV9 product: columns: ColList
	w.PutArrayLen(uint32(len(c.columns)))
	for _, col := range c.columns {
		w.PutU16(col)
	}
}
