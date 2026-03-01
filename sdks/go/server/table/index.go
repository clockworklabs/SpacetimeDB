package table

// IndexId is a numeric handle for a SpacetimeDB index.
type IndexId uint32

// UniqueIndex provides lookup by a unique column.
type UniqueIndex[R any, K any] interface {
	FindBy(key K) (R, bool, error)
	DeleteBy(key K) (bool, error)
	UpdateBy(key K, row R) (R, error)
}

// BTreeIndex provides range scanning.
type BTreeIndex[R any, K any] interface {
	Scan() (Iterator[R], error)
	ScanRange(start, end K) (Iterator[R], error)
}
