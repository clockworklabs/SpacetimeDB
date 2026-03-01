package table

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/sys"
)

// NewTable creates a table handle. Called during module init.
func NewTable[R any](name string, encode EncodeFn[R], decode DecodeFn[R]) Table[R] {
	return &tableImpl[R]{
		name:   name,
		encode: encode,
		decode: decode,
	}
}

type tableImpl[R any] struct {
	name     string
	tableId  TableId
	encode   EncodeFn[R]
	decode   DecodeFn[R]
	resolved bool
}

func (t *tableImpl[R]) resolve() error {
	if t.resolved {
		return nil
	}
	id, err := sys.TableIdFromName(t.name)
	if err != nil {
		return err
	}
	t.tableId = TableId(id)
	t.resolved = true
	return nil
}

func (t *tableImpl[R]) TableId() TableId { return t.tableId }

func (t *tableImpl[R]) Insert(row R) (R, error) {
	if err := t.resolve(); err != nil {
		var zero R
		return zero, err
	}
	rowBytes := t.encode(row)
	seqBytes, err := sys.DatastoreInsertBSATN(uint32(t.tableId), rowBytes)
	if err != nil {
		var zero R
		return zero, err
	}
	if len(seqBytes) > 0 {
		r := bsatn.NewReader(seqBytes)
		updated, decErr := t.decode(r)
		if decErr != nil {
			var zero R
			return zero, decErr
		}
		return updated, nil
	}
	return row, nil
}

func (t *tableImpl[R]) Delete(row R) error {
	if err := t.resolve(); err != nil {
		return err
	}
	rowBytes := t.encode(row)
	_, err := sys.DatastoreDeleteAllByEqBSATN(uint32(t.tableId), rowBytes)
	return err
}

func (t *tableImpl[R]) Scan() (Iterator[R], error) {
	if err := t.resolve(); err != nil {
		return nil, err
	}
	iter, err := sys.DatastoreTableScanBSATN(uint32(t.tableId))
	if err != nil {
		return nil, err
	}
	return &rowIterator[R]{
		sysIter: iter,
		decode:  t.decode,
	}, nil
}

func (t *tableImpl[R]) Count() (uint64, error) {
	if err := t.resolve(); err != nil {
		return 0, err
	}
	return sys.DatastoreTableRowCount(uint32(t.tableId))
}

// rowIterator wraps sys.RowIterator with type-safe decoding.
type rowIterator[R any] struct {
	sysIter *sys.RowIterator
	decode  DecodeFn[R]
}

func (ri *rowIterator[R]) Next() (R, bool) {
	data, ok, err := ri.sysIter.Next()
	if !ok || err != nil {
		var zero R
		return zero, false
	}
	r := bsatn.NewReader(data)
	val, err := ri.decode(r)
	if err != nil {
		var zero R
		return zero, false
	}
	return val, true
}

func (ri *rowIterator[R]) Close() {
	ri.sysIter.Close()
}

// NewUniqueIndex creates a unique index handle.
func NewUniqueIndex[R any, K any](indexName string, tbl Table[R], encodeRow EncodeFn[R], encodeKey func(K) []byte, decode DecodeFn[R]) UniqueIndex[R, K] {
	return &uniqueIndex[R, K]{
		indexName: indexName,
		tbl:       tbl,
		encodeRow: encodeRow,
		encodeKey: encodeKey,
		decode:    decode,
	}
}

type uniqueIndex[R any, K any] struct {
	indexName string
	indexId   IndexId
	tbl       Table[R]
	encodeRow EncodeFn[R]
	encodeKey func(K) []byte
	decode    DecodeFn[R]
	resolved  bool
}

func (u *uniqueIndex[R, K]) resolve() error {
	if u.resolved {
		return nil
	}
	id, err := sys.IndexIdFromName(u.indexName)
	if err != nil {
		return err
	}
	u.indexId = IndexId(id)
	u.resolved = true
	return nil
}

func (u *uniqueIndex[R, K]) FindBy(key K) (R, bool, error) {
	if err := u.resolve(); err != nil {
		var zero R
		return zero, false, err
	}
	keyBytes := u.encodeKey(key)
	iter, err := sys.DatastoreIndexScanPointBSATN(uint32(u.indexId), keyBytes)
	if err != nil {
		var zero R
		return zero, false, err
	}
	defer iter.Close()

	data, ok, err := iter.Next()
	if !ok || err != nil {
		var zero R
		return zero, false, err
	}
	r := bsatn.NewReader(data)
	val, decErr := u.decode(r)
	if decErr != nil {
		var zero R
		return zero, false, decErr
	}
	return val, true, nil
}

func (u *uniqueIndex[R, K]) DeleteBy(key K) (bool, error) {
	if err := u.resolve(); err != nil {
		return false, err
	}
	keyBytes := u.encodeKey(key)
	deleted, err := sys.DatastoreDeleteByIndexScanPointBSATN(uint32(u.indexId), keyBytes)
	if err != nil {
		return false, err
	}
	return deleted > 0, nil
}

func (u *uniqueIndex[R, K]) UpdateBy(key K, row R) (R, error) {
	if err := u.resolve(); err != nil {
		var zero R
		return zero, err
	}
	rowBytes := u.encodeRow(row)
	seqBytes, err := sys.DatastoreUpdateBSATN(uint32(u.tbl.TableId()), uint32(u.indexId), rowBytes)
	if err != nil {
		var zero R
		return zero, err
	}
	if len(seqBytes) > 0 {
		r := bsatn.NewReader(seqBytes)
		updated, decErr := u.decode(r)
		if decErr != nil {
			var zero R
			return zero, decErr
		}
		return updated, nil
	}
	return row, nil
}

// NewBTreeIndex creates a BTree index handle for range scanning.
func NewBTreeIndex[R any, K any](indexName string, encodeKey func(K) []byte, decode DecodeFn[R]) BTreeIndex[R, K] {
	return &btreeIndex[R, K]{
		indexName: indexName,
		encodeKey: encodeKey,
		decode:    decode,
	}
}

type btreeIndex[R any, K any] struct {
	indexName string
	indexId   IndexId
	encodeKey func(K) []byte
	decode    DecodeFn[R]
	resolved  bool
}

func (b *btreeIndex[R, K]) resolve() error {
	if b.resolved {
		return nil
	}
	id, err := sys.IndexIdFromName(b.indexName)
	if err != nil {
		return err
	}
	b.indexId = IndexId(id)
	b.resolved = true
	return nil
}

func (b *btreeIndex[R, K]) Scan() (Iterator[R], error) {
	if err := b.resolve(); err != nil {
		return nil, err
	}
	iter, err := sys.DatastoreIndexScanRangeBSATN(uint32(b.indexId), nil, 0, nil, nil)
	if err != nil {
		return nil, err
	}
	return &rowIterator[R]{
		sysIter: iter,
		decode:  b.decode,
	}, nil
}

func (b *btreeIndex[R, K]) ScanRange(start, end K) (Iterator[R], error) {
	if err := b.resolve(); err != nil {
		return nil, err
	}
	startBytes := b.encodeKey(start)
	endBytes := b.encodeKey(end)
	iter, err := sys.DatastoreIndexScanRangeBSATN(uint32(b.indexId), nil, 0, startBytes, endBytes)
	if err != nil {
		return nil, err
	}
	return &rowIterator[R]{
		sysIter: iter,
		decode:  b.decode,
	}, nil
}
