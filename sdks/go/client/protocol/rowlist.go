package protocol

import "github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"

// RowSizeHint describes how to determine row boundaries within packed row data.
// It is a BSATN sum type: tag 0 = FixedSize(u16), tag 1 = RowOffsets(Vec<u64>).
type RowSizeHint interface {
	isRowSizeHint()
}

// FixedSizeHint indicates all rows in the list have the same fixed byte size.
type FixedSizeHint struct {
	RowSize uint16
}

func (FixedSizeHint) isRowSizeHint() {}

// RowOffsetsHint provides byte offsets into RowsData for each row's start position.
// The end of each row is inferred from the start of the next row, or the end of RowsData.
type RowOffsetsHint struct {
	Offsets []uint64
}

func (RowOffsetsHint) isRowSizeHint() {}

// BsatnRowList holds a packed list of BSATN-encoded rows with boundary metadata.
type BsatnRowList struct {
	SizeHint RowSizeHint
	RowsData []byte
}

// ReadBsatnRowList reads a BsatnRowList from a BSATN reader.
func ReadBsatnRowList(r bsatn.Reader) (*BsatnRowList, error) {
	tag, err := r.GetSumTag()
	if err != nil {
		return nil, err
	}

	var hint RowSizeHint
	switch tag {
	case 0: // FixedSize(u16)
		size, err := r.GetU16()
		if err != nil {
			return nil, err
		}
		hint = FixedSizeHint{RowSize: size}
	case 1: // RowOffsets(Vec<u64>)
		count, err := r.GetArrayLen()
		if err != nil {
			return nil, err
		}
		offsets := make([]uint64, count)
		for i := uint32(0); i < count; i++ {
			offsets[i], err = r.GetU64()
			if err != nil {
				return nil, err
			}
		}
		hint = RowOffsetsHint{Offsets: offsets}
	default:
		return nil, &bsatn.ErrInvalidTag{Tag: tag, SumName: "RowSizeHint"}
	}

	// rows_data: Bytes = u32 len + raw bytes
	data, err := bsatn.ReadByteArray(r)
	if err != nil {
		return nil, err
	}

	return &BsatnRowList{SizeHint: hint, RowsData: data}, nil
}

// Rows returns individual row byte slices extracted using the size hint.
func (rl *BsatnRowList) Rows() [][]byte {
	if rl == nil || len(rl.RowsData) == 0 {
		return nil
	}

	switch h := rl.SizeHint.(type) {
	case FixedSizeHint:
		if h.RowSize == 0 {
			return nil
		}
		size := int(h.RowSize)
		var rows [][]byte
		for i := 0; i+size <= len(rl.RowsData); i += size {
			rows = append(rows, rl.RowsData[i:i+size])
		}
		return rows
	case RowOffsetsHint:
		rows := make([][]byte, 0, len(h.Offsets))
		for i, offset := range h.Offsets {
			start := int(offset)
			var end int
			if i+1 < len(h.Offsets) {
				end = int(h.Offsets[i+1])
			} else {
				end = len(rl.RowsData)
			}
			if start <= len(rl.RowsData) && end <= len(rl.RowsData) && start <= end {
				rows = append(rows, rl.RowsData[start:end])
			}
		}
		return rows
	}
	return nil
}

// Len returns the number of rows in the list.
func (rl *BsatnRowList) Len() int {
	if rl == nil || len(rl.RowsData) == 0 {
		return 0
	}

	switch h := rl.SizeHint.(type) {
	case FixedSizeHint:
		if h.RowSize == 0 {
			return 0
		}
		return len(rl.RowsData) / int(h.RowSize)
	case RowOffsetsHint:
		return len(h.Offsets)
	}
	return 0
}
