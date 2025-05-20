package db

import (
	"sync"
)

// Error codes
const (
	ErrNoSuchTable = 1
	ErrNoSuchIndex = 2
	INVALID        = 0xFFFFFFFF
)

// TableID represents a table identifier
type TableID uint32

// IndexID represents an index identifier
type IndexID uint32

// RowIter represents an iterator over table rows
type RowIter uint32

// Table represents a database table
type Table interface {
	GetID() TableID
	GetName() string
	GetSchema() []byte
	Insert(data []byte) error
	Update(key []byte, value []byte) error
	Delete(key []byte) error
	Scan() (RowIter, error)
}

// Index represents a database index
type Index interface {
	GetID() IndexID
	GetName() string
	GetTableID() TableID
	GetAlgorithm() string
	Scan(lower []byte, upper []byte, lowerInclusive bool, upperInclusive bool, limit uint32, offset uint32) (RowIter, error)
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
}

// NewDatabase creates a new database instance
func NewDatabase() (*Database, error) {
	return &Database{
		tables:  make(map[TableID]*TableImpl),
		indices: make(map[IndexID]*IndexImpl),
	}, nil
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
	return table, nil
}

// GetTableByName returns a table by name
func (db *Database) GetTableByName(name string) (Table, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()

	for _, table := range db.tables {
		if table.GetName() == name {
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
	return index, nil
}

// GetIndexByName returns an index by name
func (db *Database) GetIndexByName(name string) (Index, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()

	for _, index := range db.indices {
		if index.GetName() == name {
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

// Scan scans a table
func (db *Database) Scan(tableID TableID) (RowIter, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()

	table, ok := db.tables[tableID]
	if !ok {
		return RowIter(INVALID), NewErrno(ErrNoSuchTable)
	}

	return table.Scan()
}

// ScanIndex scans an index
func (db *Database) ScanIndex(indexID IndexID, lower []byte, upper []byte, lowerInclusive bool, upperInclusive bool, limit uint32, offset uint32) (RowIter, error) {
	db.mu.RLock()
	defer db.mu.RUnlock()

	index, ok := db.indices[indexID]
	if !ok {
		return RowIter(INVALID), NewErrno(ErrNoSuchIndex)
	}

	return index.Scan(lower, upper, lowerInclusive, upperInclusive, limit, offset)
}

// TableImpl implements the Table interface
type TableImpl struct {
	id     TableID
	name   string
	schema []byte
	data   map[string][]byte // key -> value
}

// GetID returns the table's ID
func (t *TableImpl) GetID() TableID {
	return t.id
}

// GetName returns the table's name
func (t *TableImpl) GetName() string {
	return t.name
}

// GetSchema returns the table's schema
func (t *TableImpl) GetSchema() []byte {
	return t.schema
}

// Insert inserts a row into the table
func (t *TableImpl) Insert(data []byte) error {
	// TODO: Implement actual insertion logic
	return nil
}

// Update updates rows in the table
func (t *TableImpl) Update(key []byte, value []byte) error {
	// TODO: Implement actual update logic
	return nil
}

// Delete deletes rows from the table
func (t *TableImpl) Delete(key []byte) error {
	// TODO: Implement actual deletion logic
	return nil
}

// Scan returns an iterator over the table's rows
func (t *TableImpl) Scan() (RowIter, error) {
	// TODO: Implement actual scan logic
	return RowIter(INVALID), nil
}

// IndexImpl implements the Index interface
type IndexImpl struct {
	id        IndexID
	name      string
	tableID   TableID
	algorithm string
	data      map[string][]string // key -> list of row keys
}

// GetID returns the index's ID
func (i *IndexImpl) GetID() IndexID {
	return i.id
}

// GetName returns the index's name
func (i *IndexImpl) GetName() string {
	return i.name
}

// GetTableID returns the index's table ID
func (i *IndexImpl) GetTableID() TableID {
	return i.tableID
}

// GetAlgorithm returns the index's algorithm
func (i *IndexImpl) GetAlgorithm() string {
	return i.algorithm
}

// Scan returns an iterator over the index's entries
func (i *IndexImpl) Scan(lower []byte, upper []byte, lowerInclusive bool, upperInclusive bool, limit uint32, offset uint32) (RowIter, error) {
	// TODO: Implement actual scan logic
	return RowIter(INVALID), nil
}
