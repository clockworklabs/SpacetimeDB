package bsatn

// WriteArray writes a slice as a BSATN array: u32 LE count + each element.
func WriteArray[T Serializable](w Writer, items []T) {
	w.PutArrayLen(uint32(len(items)))
	for i := range items {
		items[i].WriteBsatn(w)
	}
}

// ReadArray reads a BSATN array using the provided element read function.
func ReadArray[T any](r Reader, readFn func(Reader) (T, error)) ([]T, error) {
	count, err := r.GetArrayLen()
	if err != nil {
		return nil, err
	}
	items := make([]T, 0, count)
	for i := uint32(0); i < count; i++ {
		item, err := readFn(r)
		if err != nil {
			return nil, err
		}
		items = append(items, item)
	}
	return items, nil
}

// WriteByteArray writes a byte slice as BSATN: u32 LE length + raw bytes.
func WriteByteArray(w Writer, data []byte) {
	w.PutArrayLen(uint32(len(data)))
	w.PutBytes(data)
}

// ReadByteArray reads a BSATN byte array.
func ReadByteArray(r Reader) ([]byte, error) {
	count, err := r.GetArrayLen()
	if err != nil {
		return nil, err
	}
	return r.GetBytes(int(count))
}
