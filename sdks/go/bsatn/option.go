package bsatn

// WriteOption writes an Option<T> value as a BSATN sum type.
// Some(value) = tag 0 + encoded value
// None = tag 1 (empty product)
func WriteOption[T Serializable](w Writer, v *T) {
	if v != nil {
		w.PutSumTag(0)
		(*v).WriteBsatn(w)
	} else {
		w.PutSumTag(1)
	}
}

// ReadOption reads a BSATN Option<T> using the provided read function.
func ReadOption[T any](r Reader, readFn func(Reader) (T, error)) (*T, error) {
	tag, err := r.GetSumTag()
	if err != nil {
		return nil, err
	}
	switch tag {
	case 0: // Some
		v, err := readFn(r)
		if err != nil {
			return nil, err
		}
		return &v, nil
	case 1: // None
		return nil, nil
	default:
		return nil, &ErrInvalidTag{Tag: tag, SumName: "Option"}
	}
}
