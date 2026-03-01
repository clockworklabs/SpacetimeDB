package sys

const bytesSourceInvalid = uint32(0)

// ReadBytesSource reads all data from a BytesSource handle.
func ReadBytesSource(source uint32) ([]byte, error) {
	if source == bytesSourceInvalid {
		return nil, nil
	}

	// Try to get remaining length for pre-allocation
	var remaining uint32
	ret := rawBytesSourceRemainingLength(source, &remaining)

	var buf []byte
	if ret == 0 && remaining > 0 {
		buf = make([]byte, 0, remaining)
	} else {
		buf = make([]byte, 0, 1024)
	}

	for {
		spare := cap(buf) - len(buf)
		if spare == 0 {
			buf = append(buf, make([]byte, 1024)...)
			buf = buf[:len(buf)-1024]
			spare = cap(buf) - len(buf)
		}

		bufLen := uint32(spare)
		ptr := &buf[len(buf):cap(buf)][0]
		ret := rawBytesSourceRead(source, ptr, &bufLen)

		buf = buf[:len(buf)+int(bufLen)]

		switch {
		case ret == -1:
			return buf, nil // exhausted
		case ret == 0:
			continue
		default:
			return nil, Errno(uint16(int16(ret)))
		}
	}
}
