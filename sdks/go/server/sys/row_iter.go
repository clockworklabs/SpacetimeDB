package sys

const rowIterInvalid = uint32(0)

// RowIterator wraps a host row iterator handle with buffer management.
type RowIterator struct {
	handle uint32
	buf    []byte
	done   bool
}

// NewRowIterator creates a new row iterator from a host handle.
func NewRowIterator(handle uint32) *RowIterator {
	return &RowIterator{
		handle: handle,
		buf:    make([]byte, 0, 4096),
		done:   handle == rowIterInvalid,
	}
}

// IsExhausted returns true if the host iterator has been fully consumed.
func (ri *RowIterator) IsExhausted() bool {
	return ri.done
}

// ReadBatch reads the next batch of BSATN-encoded rows from the host iterator
// and appends them to buf. The host may write multiple rows packed sequentially
// into a single batch. Returns any error from the host.
func (ri *RowIterator) ReadBatch(buf *[]byte) error {
	if ri.done {
		return nil
	}

	for {
		ri.buf = ri.buf[:cap(ri.buf)]
		bufLen := uint32(len(ri.buf))
		var ptr *byte
		if bufLen > 0 {
			ptr = &ri.buf[0]
		}

		ret := rawRowIterBSATNAdvance(ri.handle, ptr, &bufLen)

		switch {
		case ret == -1:
			ri.done = true
			if bufLen > 0 {
				*buf = append(*buf, ri.buf[:bufLen]...)
			}
			return nil
		case ret == 0:
			*buf = append(*buf, ri.buf[:bufLen]...)
			return nil
		case ret == int32(ErrBufferTooSmall):
			// bufLen now contains the needed size
			ri.buf = make([]byte, bufLen)
			continue
		default:
			return Errno(uint16(ret))
		}
	}
}

// Next reads the next batch from the iterator and returns all bytes as a single blob.
// This is suitable for single-row results (e.g., FindBy point scans).
// For multi-row iteration, use ReadBatch with a cursor-based approach instead.
func (ri *RowIterator) Next() ([]byte, bool, error) {
	if ri.done {
		return nil, false, nil
	}

	for {
		ri.buf = ri.buf[:cap(ri.buf)]
		bufLen := uint32(len(ri.buf))
		var ptr *byte
		if bufLen > 0 {
			ptr = &ri.buf[0]
		}

		ret := rawRowIterBSATNAdvance(ri.handle, ptr, &bufLen)

		switch {
		case ret == -1:
			ri.done = true
			if bufLen > 0 {
				row := make([]byte, bufLen)
				copy(row, ri.buf[:bufLen])
				return row, true, nil
			}
			return nil, false, nil
		case ret == 0:
			row := make([]byte, bufLen)
			copy(row, ri.buf[:bufLen])
			return row, true, nil
		case ret == int32(ErrBufferTooSmall):
			// bufLen now contains the needed size
			ri.buf = make([]byte, bufLen)
			continue
		default:
			return nil, false, Errno(uint16(ret))
		}
	}
}

// Close releases the iterator handle.
func (ri *RowIterator) Close() {
	if !ri.done && ri.handle != rowIterInvalid {
		rawRowIterBSATNClose(ri.handle)
		ri.done = true
	}
}
