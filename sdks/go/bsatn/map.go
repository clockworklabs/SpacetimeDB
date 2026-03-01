package bsatn

// WriteMap writes a map as BSATN: u32 LE count + key-value pairs.
// Note: Go maps have non-deterministic iteration order. For deterministic output,
// callers should use sorted key iteration.
func WriteMap[K Serializable, V Serializable](w Writer, items []struct {
	Key   K
	Value V
}) {
	w.PutMapLen(uint32(len(items)))
	for _, item := range items {
		item.Key.WriteBsatn(w)
		item.Value.WriteBsatn(w)
	}
}

// ReadMap reads a BSATN map using provided key/value read functions.
func ReadMap[K comparable, V any](r Reader, readK func(Reader) (K, error), readV func(Reader) (V, error)) (map[K]V, error) {
	count, err := r.GetMapLen()
	if err != nil {
		return nil, err
	}
	m := make(map[K]V, count)
	for i := uint32(0); i < count; i++ {
		k, err := readK(r)
		if err != nil {
			return nil, err
		}
		v, err := readV(r)
		if err != nil {
			return nil, err
		}
		m[k] = v
	}
	return m, nil
}
