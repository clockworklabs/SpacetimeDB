package moduledef

import "github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"

// IndexAlgo defines the index algorithm.
// BSATN enum: BTree=0, Hash=1, Direct=2.
type IndexAlgo uint8

const (
	IndexAlgoBTree  IndexAlgo = 0
	IndexAlgoHash   IndexAlgo = 1
	IndexAlgoDirect IndexAlgo = 2
)

// IndexDef defines an index on a table.
type IndexDef interface {
	bsatn.Serializable
}

// IndexDefBuilder builds an IndexDef.
type IndexDefBuilder interface {
	WithAccessorName(name string) IndexDefBuilder
	Build() IndexDef
}

// NewBTreeIndexDef creates a BTree index definition.
// sourceName is optional (nil for auto-generated). columns are the column IDs.
func NewBTreeIndexDef(sourceName *string, columns ...uint16) IndexDefBuilder {
	return &indexDef{
		sourceName: sourceName,
		algo:       IndexAlgoBTree,
		columns:    columns,
	}
}

// NewHashIndexDef creates a Hash index definition.
func NewHashIndexDef(sourceName *string, columns ...uint16) IndexDefBuilder {
	return &indexDef{
		sourceName: sourceName,
		algo:       IndexAlgoHash,
		columns:    columns,
	}
}

// NewDirectIndexDef creates a Direct index definition.
func NewDirectIndexDef(sourceName *string, column uint16) IndexDefBuilder {
	return &indexDef{
		sourceName: sourceName,
		algo:       IndexAlgoDirect,
		columns:    []uint16{column},
	}
}

type indexDef struct {
	sourceName   *string
	accessorName *string
	algo         IndexAlgo
	columns      []uint16
}

func (d *indexDef) WithAccessorName(name string) IndexDefBuilder {
	d.accessorName = &name
	return d
}

func (d *indexDef) Build() IndexDef {
	return d
}

// WriteBsatn encodes the index definition as BSATN.
//
// Matches RawIndexDefV10 product field order:
//
//	source_name: Option<String>
//	accessor_name: Option<String>
//	algorithm: RawIndexAlgorithm (sum type)
func (d *indexDef) WriteBsatn(w bsatn.Writer) {
	// source_name: Option<String>
	writeOptionString(w, d.sourceName)

	// accessor_name: Option<String>
	writeOptionString(w, d.accessorName)

	// algorithm: RawIndexAlgorithm (sum type)
	// BTree=0 { columns: ColList }, Hash=1 { columns: ColList }, Direct=2 { column: ColId }
	w.PutSumTag(uint8(d.algo))
	switch d.algo {
	case IndexAlgoBTree, IndexAlgoHash:
		// columns: ColList (array of u16)
		w.PutArrayLen(uint32(len(d.columns)))
		for _, col := range d.columns {
			w.PutU16(col)
		}
	case IndexAlgoDirect:
		// column: ColId (u16)
		if len(d.columns) > 0 {
			w.PutU16(d.columns[0])
		} else {
			w.PutU16(0)
		}
	}
}

// writeOptionString writes an Option<String> as BSATN.
func writeOptionString(w bsatn.Writer, s *string) {
	if s != nil {
		w.PutSumTag(0) // Some
		w.PutString(*s)
	} else {
		w.PutSumTag(1) // None
	}
}
