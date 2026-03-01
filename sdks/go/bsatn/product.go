package bsatn

// Products are encoded as sequential fields with no length prefix.
// Each field is encoded according to its type, one after another.
// There is no special wrapper - product encoding is implicit in
// the WriteBsatn implementation of each struct type.
//
// Example:
//
//	func (p *Person) WriteBsatn(w Writer) {
//	    w.PutU32(p.ID)
//	    w.PutString(p.Name)
//	    w.PutU8(p.Age)
//	}
