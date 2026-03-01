package moduledef

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// TableAccess defines table visibility.
// BSATN enum: Public=0, Private=1.
type TableAccess uint8

const (
	TableAccessPublic  TableAccess = 0
	TableAccessPrivate TableAccess = 1
)

// TableType defines table category.
// BSATN enum: System=0, User=1.
type TableType uint8

const (
	TableTypeSystem TableType = 0
	TableTypeUser   TableType = 1
)

// TableDef defines a table in the module.
type TableDef interface {
	bsatn.Serializable
}

// TableDefBuilder builds a TableDef.
type TableDefBuilder interface {
	WithProductTypeRef(ref types.TypeRef) TableDefBuilder
	WithPrimaryKey(cols ...uint16) TableDefBuilder
	WithIndex(idx IndexDef) TableDefBuilder
	WithConstraint(c ConstraintDef) TableDefBuilder
	WithSequence(s SequenceDef) TableDefBuilder
	WithTableType(tt TableType) TableDefBuilder
	WithTableAccess(ta TableAccess) TableDefBuilder
	WithDefaultValue(dv ColumnDefaultValue) TableDefBuilder
	WithIsEvent(isEvent bool) TableDefBuilder
	Build() TableDef
}

// NewTableDefBuilder creates a TableDefBuilder with the given source name.
func NewTableDefBuilder(sourceName string) TableDefBuilder {
	return &tableDef{
		sourceName:  sourceName,
		tableType:   TableTypeUser,
		tableAccess: TableAccessPublic,
	}
}

type tableDef struct {
	sourceName     string
	productTypeRef types.TypeRef
	primaryKey     []uint16
	indexes        []IndexDef
	constraints    []ConstraintDef
	sequences      []SequenceDef
	tableType      TableType
	tableAccess    TableAccess
	defaultValues  []ColumnDefaultValue
	isEvent        bool
}

func (t *tableDef) WithProductTypeRef(ref types.TypeRef) TableDefBuilder {
	t.productTypeRef = ref
	return t
}

func (t *tableDef) WithPrimaryKey(cols ...uint16) TableDefBuilder {
	t.primaryKey = cols
	return t
}

func (t *tableDef) WithIndex(idx IndexDef) TableDefBuilder {
	t.indexes = append(t.indexes, idx)
	return t
}

func (t *tableDef) WithConstraint(c ConstraintDef) TableDefBuilder {
	t.constraints = append(t.constraints, c)
	return t
}

func (t *tableDef) WithSequence(s SequenceDef) TableDefBuilder {
	t.sequences = append(t.sequences, s)
	return t
}

func (t *tableDef) WithTableType(tt TableType) TableDefBuilder {
	t.tableType = tt
	return t
}

func (t *tableDef) WithTableAccess(ta TableAccess) TableDefBuilder {
	t.tableAccess = ta
	return t
}

func (t *tableDef) WithDefaultValue(dv ColumnDefaultValue) TableDefBuilder {
	t.defaultValues = append(t.defaultValues, dv)
	return t
}

func (t *tableDef) WithIsEvent(isEvent bool) TableDefBuilder {
	t.isEvent = isEvent
	return t
}

func (t *tableDef) Build() TableDef {
	return t
}

// WriteBsatn encodes the table definition as BSATN.
//
// Matches RawTableDefV10 product field order:
//
//	source_name: String
//	product_type_ref: AlgebraicTypeRef (u32)
//	primary_key: ColList (array of u16)
//	indexes: Vec<RawIndexDefV10>
//	constraints: Vec<RawConstraintDefV10>
//	sequences: Vec<RawSequenceDefV10>
//	table_type: TableType (sum tag)
//	table_access: TableAccess (sum tag)
//	default_values: Vec<RawColumnDefaultValueV10>
//	is_event: bool
func (t *tableDef) WriteBsatn(w bsatn.Writer) {
	// source_name: String
	w.PutString(t.sourceName)

	// product_type_ref: AlgebraicTypeRef (u32)
	w.PutU32(uint32(t.productTypeRef))

	// primary_key: ColList (serialized as array of u16)
	w.PutArrayLen(uint32(len(t.primaryKey)))
	for _, col := range t.primaryKey {
		w.PutU16(col)
	}

	// indexes: Vec<RawIndexDefV10>
	w.PutArrayLen(uint32(len(t.indexes)))
	for _, idx := range t.indexes {
		idx.WriteBsatn(w)
	}

	// constraints: Vec<RawConstraintDefV10>
	w.PutArrayLen(uint32(len(t.constraints)))
	for _, c := range t.constraints {
		c.WriteBsatn(w)
	}

	// sequences: Vec<RawSequenceDefV10>
	w.PutArrayLen(uint32(len(t.sequences)))
	for _, s := range t.sequences {
		s.WriteBsatn(w)
	}

	// table_type: TableType (sum tag)
	w.PutSumTag(uint8(t.tableType))

	// table_access: TableAccess (sum tag)
	w.PutSumTag(uint8(t.tableAccess))

	// default_values: Vec<RawColumnDefaultValueV10>
	w.PutArrayLen(uint32(len(t.defaultValues)))
	for _, dv := range t.defaultValues {
		dv.WriteBsatn(w)
	}

	// is_event: bool
	w.PutBool(t.isEvent)
}

// ColumnDefaultValue marks a column as having a default value.
type ColumnDefaultValue interface {
	bsatn.Serializable
}

// NewColumnDefaultValue creates a ColumnDefaultValue for a specific column.
// The value is pre-encoded BSATN bytes of the default AlgebraicValue.
func NewColumnDefaultValue(colID uint16, value []byte) ColumnDefaultValue {
	return &columnDefaultValue{colID: colID, value: value}
}

type columnDefaultValue struct {
	colID uint16
	value []byte
}

// WriteBsatn encodes: col_id (u16), value (Vec<u8>).
func (d *columnDefaultValue) WriteBsatn(w bsatn.Writer) {
	w.PutU16(d.colID)
	w.PutArrayLen(uint32(len(d.value)))
	w.PutBytes(d.value)
}
