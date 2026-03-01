package bsatn

// WriteSum writes a sum type value: u8 tag + payload.
func WriteSum(w Writer, tag uint8, payload Serializable) {
	w.PutSumTag(tag)
	if payload != nil {
		payload.WriteBsatn(w)
	}
}

// WriteSumUnit writes a sum type with a unit (empty product) variant.
func WriteSumUnit(w Writer, tag uint8) {
	w.PutSumTag(tag)
}
