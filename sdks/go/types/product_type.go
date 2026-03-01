package types

import "github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"

// ProductTypeElement is a named field in a product type.
type ProductTypeElement struct {
	Name          string
	AlgebraicType AlgebraicType
}

// ProductType describes a product (struct) type with named fields.
type ProductType interface {
	bsatn.Serializable
	Elements() []ProductTypeElement
}

// NewProductType creates a ProductType from the given elements.
func NewProductType(elements ...ProductTypeElement) ProductType {
	return &productType{elements: elements}
}

type productType struct {
	elements []ProductTypeElement
}

func (p *productType) Elements() []ProductTypeElement {
	return p.elements
}

func (p *productType) WriteBsatn(w bsatn.Writer) {
	// ProductType is encoded as array of ProductTypeElement.
	w.PutArrayLen(uint32(len(p.elements)))
	for _, elem := range p.elements {
		// Each element is a product of (name: Option<String>, algebraic_type: AlgebraicType).
		// Name is always present (Some).
		w.PutSumTag(0) // Some
		w.PutString(elem.Name)
		elem.AlgebraicType.WriteBsatn(w)
	}
}
