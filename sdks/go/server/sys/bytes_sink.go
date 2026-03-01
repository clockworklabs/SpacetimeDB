package sys

// WriteBytesToSink writes all data to a BytesSink handle.
func WriteBytesToSink(sink uint32, data []byte) error {
	for len(data) > 0 {
		bufLen := uint32(len(data))
		ret := rawBytesSinkWrite(sink, &data[0], &bufLen)
		if ret != 0 {
			return Errno(uint16(ret))
		}
		data = data[bufLen:]
	}
	return nil
}
