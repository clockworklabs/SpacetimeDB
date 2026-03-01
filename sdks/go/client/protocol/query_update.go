package protocol

// QueryRows holds the matching rows for a set of tables,
// used in contexts like SubscribeApplied and OneOffQueryResult.
type QueryRows struct {
	Tables []SingleTableRows
}

// SingleTableRows holds the matching rows from a single table.
type SingleTableRows struct {
	TableName string
	Rows      *BsatnRowList
}

// QuerySetUpdate describes the changes to a single query set
// as part of a TransactionUpdate.
type QuerySetUpdate struct {
	QuerySetID uint32
	Tables     []TableUpdate
}

// TableUpdate describes the row changes for a single table.
type TableUpdate struct {
	TableName string
	Rows      []TableUpdateRows
}

// TableUpdateRows is a sum type representing either persistent table
// insert/delete rows or event table rows.
// Tag 0 = PersistentTableRows, Tag 1 = EventTableRows.
type TableUpdateRows interface {
	isTableUpdateRows()
}

// PersistentTableRows holds inserted and deleted rows for a persistent table.
type PersistentTableRows struct {
	Inserts *BsatnRowList
	Deletes *BsatnRowList
}

func (*PersistentTableRows) isTableUpdateRows() {}

// EventTableRows holds event rows for an event table.
type EventTableRows struct {
	Events *BsatnRowList
}

func (*EventTableRows) isTableUpdateRows() {}
