package db

import (
	"fmt"
	"sync"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/bsatn"
	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/runtime"
)

// Error codes
const (
	ErrNoSuchTable   = 1
	ErrNoSuchIndex   = 2
	INVALID          = 0xFFFFFFFF
	BUFFER_TOO_SMALL = 0xFFFFFFFE
	NEGATIVE_ONE     = 0xFFFF
)

// TableID represents a table identifier
type TableID uint32

// IndexID represents an index identifier
type IndexID uint32

// RowIter represents an iterator over table rows
type RowIter struct {
	data [][]byte
	idx  int
	// legacy fields for compatibility with wasm layer (unused in in-memory mode)
	IterID  uint32
	Runtime *runtime.Runtime
}

// Read reads the next row from the iterator. Returns error when exhausted.
func (iter *RowIter) Read() ([]byte, error) {
	if iter.IsExhausted() {
		return nil, fmt.Errorf("iterator exhausted")
	}
	row := iter.data[iter.idx]
	iter.idx++
	return row, nil
}

// IsExhausted checks if the iterator is exhausted
func (iter *RowIter) IsExhausted() bool {
	return iter.idx >= len(iter.data)
}

// Close is a no-op for in-memory iterator
func (iter *RowIter) Close() error { return nil }

// Table represents a database table
type Table interface {
	GetID() TableID
	GetName() string
	GetSchema() []byte
	Insert(data []byte) error
	Update(key []byte, value []byte) error
	Delete(key []byte) error
	Scan() (*RowIter, error)
}

// Index represents a database index
type Index interface {
	GetID() IndexID
	GetName() string
	GetTableID() TableID
	GetAlgorithm() string
	Scan(lower []byte, upper []byte, lowerInclusive bool, upperInclusive bool, limit uint32, offset uint32) (*RowIter, error)
}

// Errno represents a database error
type Errno struct {
	Code uint16
}

func (e *Errno) Error() string {
	switch e.Code {
	case ErrNoSuchTable:
		return "no such table"
	case ErrNoSuchIndex:
		return "no such index"
	default:
		return "unknown error"
	}
}

// NewErrno creates a new database error
func NewErrno(code uint16) *Errno {
	return &Errno{Code: code}
}

// Database represents a SpacetimeDB database instance
type Database struct {
	mu      sync.RWMutex
	tables  map[TableID]*TableImpl
	indices map[IndexID]*IndexImpl
	runtime *runtime.Runtime
}

// NewDatabase creates a new database instance
func NewDatabase(runtime *runtime.Runtime) (*Database, error) {
	return &Database{
		tables:  make(map[TableID]*TableImpl),
		indices: make(map[IndexID]*IndexImpl),
		runtime: runtime,
	}, nil
}

// GetAllTables returns all tables in the database
func (db *Database) GetAllTables() map[TableID]*TableImpl {
	db.mu.RLock()
	defer db.mu.RUnlock()

	// Return a copy of the tables map to prevent concurrent modification
	tables := make(map[TableID]*TableImpl, len(db.tables))
	for id, table := range db.tables {
		tables[id] = table
	}

	return tables
}

// RegisterTable registers a table in the database
func (db *Database) RegisterTable(id TableID, table *TableImpl) {
	db.mu.Lock()
	defer db.mu.Unlock()

	db.tables[id] = table
}

// RegisterIndex registers an index in the database
func (db *Database) RegisterIndex(id IndexID, idx *IndexImpl) {
	db.mu.Lock()
	defer db.mu.Unlock()

	db.indices[id] = idx
}

// Serialize serializes data using BSATN format
func (db *Database) Serialize(value interface{}) ([]byte, error) {
	// If caller already gives us a byte slice, pass it through so existing
	// code that just wants to stash raw BSATN works unchanged.
	if b, ok := value.([]byte); ok {
		return b, nil
	}
	// Otherwise marshal via bsatn codec.
	return bsatn.Marshal(value)
}

// Deserialize deserializes data. For compatibility with older code paths this
// currently just passes the bytes back unchanged.  Higher-level callers can
// invoke bsatn.Unmarshal themselves when they know the concrete value type.
func (db *Database) Deserialize(data []byte) ([]byte, error) {
	return data, nil
}

// Close closes the database connection
func (db *Database) Close() error {
	return nil
}

// GetTable returns a table by ID
func (db *Database) GetTable(id TableID) (Table, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()

	table, ok := db.tables[id]
	if !ok {
		return nil, NewErrno(ErrNoSuchTable)
	}
	table.runtime = db.runtime
	return table, nil
}

// GetTableByName returns a table by name
func (db *Database) GetTableByName(name string) (Table, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()

	for _, table := range db.tables {
		if table.GetName() == name {
			table.runtime = db.runtime
			return table, nil
		}
	}
	return nil, NewErrno(ErrNoSuchTable)
}

// GetIndex returns an index by ID
func (db *Database) GetIndex(id IndexID) (Index, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()

	index, ok := db.indices[id]
	if !ok {
		return nil, NewErrno(ErrNoSuchIndex)
	}
	index.runtime = db.runtime
	return index, nil
}

// GetIndexByName returns an index by name
func (db *Database) GetIndexByName(name string) (Index, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()

	for _, index := range db.indices {
		if index.GetName() == name {
			index.runtime = db.runtime
			return index, nil
		}
	}
	return nil, NewErrno(ErrNoSuchIndex)
}

// Insert inserts a row into a table
func (db *Database) Insert(tableID TableID, data []byte) error {
	db.mu.Lock()
	defer db.mu.Unlock()

	table, ok := db.tables[tableID]
	if !ok {
		return NewErrno(ErrNoSuchTable)
	}

	return table.Insert(data)
}

// Update updates rows in a table
func (db *Database) Update(tableID TableID, key []byte, value []byte) error {
	db.mu.Lock()
	defer db.mu.Unlock()

	table, ok := db.tables[tableID]
	if !ok {
		return NewErrno(ErrNoSuchTable)
	}

	return table.Update(key, value)
}

// Delete deletes rows from a table
func (db *Database) Delete(tableID TableID, key []byte) error {
	db.mu.Lock()
	defer db.mu.Unlock()

	table, ok := db.tables[tableID]
	if !ok {
		return NewErrno(ErrNoSuchTable)
	}

	return table.Delete(key)
}

// Scan creates an iterator for scanning a table
func (db *Database) Scan(tableID TableID) (*RowIter, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()

	table, ok := db.tables[tableID]
	if !ok {
		return nil, NewErrno(ErrNoSuchTable)
	}

	return table.Scan()
}

// ScanIndex creates an iterator for scanning an index
func (db *Database) ScanIndex(indexID IndexID, lower []byte, upper []byte, lowerInclusive bool, upperInclusive bool, limit uint32, offset uint32) (*RowIter, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()

	index, ok := db.indices[indexID]
	if !ok {
		return nil, NewErrno(ErrNoSuchIndex)
	}

	return index.Scan(lower, upper, lowerInclusive, upperInclusive, limit, offset)
}

// TableImpl represents a database table implementation
type TableImpl struct {
	id      TableID
	name    string
	schema  []byte
	data    map[string][]byte // key -> value
	runtime *runtime.Runtime
	mu      sync.RWMutex
}

// NewTableImpl creates a new TableImpl instance
func NewTableImpl(id TableID, name string, schema []byte, runtime *runtime.Runtime) *TableImpl {
	return &TableImpl{
		id:      id,
		name:    name,
		schema:  schema,
		data:    make(map[string][]byte),
		runtime: runtime,
	}
}

// GetID returns the table ID
func (t *TableImpl) GetID() TableID {
	return t.id
}

// GetName returns the table name
func (t *TableImpl) GetName() string {
	return t.name
}

// GetSchema returns the table schema
func (t *TableImpl) GetSchema() []byte {
	return t.schema
}

// Insert inserts a row into the table (in-memory implementation)
func (t *TableImpl) Insert(data []byte) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	// Use the raw bytes as the key for uniqueness in this mock implementation
	t.data[string(data)] = append([]byte(nil), data...)
	return nil
}

// Update updates rows in the table (in-memory implementation)
func (t *TableImpl) Update(key []byte, value []byte) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	if _, ok := t.data[string(key)]; !ok {
		return fmt.Errorf("key not found")
	}
	t.data[string(key)] = append([]byte(nil), value...)
	return nil
}

// Delete deletes rows from the table (in-memory implementation)
func (t *TableImpl) Delete(key []byte) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	delete(t.data, string(key))
	return nil
}

// Scan creates an iterator over a snapshot of the table data
func (t *TableImpl) Scan() (*RowIter, error) {
	t.mu.RLock()
	defer t.mu.RUnlock()

	rows := make([][]byte, 0, len(t.data))
	for _, v := range t.data {
		rows = append(rows, append([]byte(nil), v...))
	}

	return &RowIter{
		data: rows,
		idx:  0,
	}, nil
}

// IndexImpl implements the Index interface
type IndexImpl struct {
	id        IndexID
	name      string
	tableID   TableID
	algorithm string
	data      map[string][]string // key -> list of row keys
	runtime   *runtime.Runtime
}

// GetID returns the index ID
func (i *IndexImpl) GetID() IndexID {
	return i.id
}

// GetName returns the index name
func (i *IndexImpl) GetName() string {
	return i.name
}

// GetTableID returns the table ID
func (i *IndexImpl) GetTableID() TableID {
	return i.tableID
}

// GetAlgorithm returns the index algorithm
func (i *IndexImpl) GetAlgorithm() string {
	return i.algorithm
}

// Scan creates an iterator for scanning the index
func (i *IndexImpl) Scan(lower []byte, upper []byte, lowerInclusive bool, upperInclusive bool, limit uint32, offset uint32) (*RowIter, error) {
	// TODO: Implement actual scanning
	return &RowIter{
		data: [][]byte{},
	}, nil
}

// NewIndexImpl creates a new IndexImpl instance
func NewIndexImpl(id IndexID, name string, tableID TableID, runtime *runtime.Runtime) *IndexImpl {
	return &IndexImpl{
		id:        id,
		name:      name,
		tableID:   tableID,
		algorithm: "btree",
		data:      make(map[string][]string),
		runtime:   runtime,
	}
}

// 3. Determine Reducer ID for "create_table"
// TODO: Implement dynamic lookup of reducer ID by name (e.g., "sptdb_create_table")
//
//	or assume a known convention. Using a placeholder ID for now.
