package bsatn

// Encode serializes a Serializable value to BSATN bytes.
func Encode(v Serializable) []byte {
	w := NewWriter(64)
	v.WriteBsatn(w)
	return w.Bytes()
}

// Decode deserializes BSATN bytes using the provided read function.
func Decode[T any](data []byte, readFn func(Reader) (T, error)) (T, error) {
	r := NewReader(data)
	return readFn(r)
}
