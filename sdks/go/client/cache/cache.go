package cache

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/client/protocol"
	"github.com/puzpuzpuz/xsync/v3"
)

// ClientCache manages all table caches.
type ClientCache interface {
	GetTable(name string) TableCache
	RegisterTable(def TableDef)
	ApplySubscribeApplied(rows *protocol.QueryRows)
	ApplyTransactionUpdate(update *protocol.TransactionUpdate)
}

func NewClientCache() ClientCache {
	return &clientCache{
		tables: xsync.NewMapOf[string, *tableCache](),
	}
}

type clientCache struct {
	tables *xsync.MapOf[string, *tableCache]
}

func (cc *clientCache) GetTable(name string) TableCache {
	tc, ok := cc.tables.Load(name)
	if !ok {
		return nil
	}
	return tc
}

func (cc *clientCache) RegisterTable(def TableDef) {
	cc.tables.Store(def.TableName(), newTableCache(def))
}

func (cc *clientCache) ApplySubscribeApplied(rows *protocol.QueryRows) {
	if rows == nil {
		return
	}
	for _, tableRows := range rows.Tables {
		tc, ok := cc.tables.Load(tableRows.TableName)
		if !ok {
			continue
		}
		if tableRows.Rows == nil {
			continue
		}
		for _, rowData := range tableRows.Rows.Rows() {
			r := bsatn.NewReader(rowData)
			row, err := tc.def.DecodeRow(r)
			if err != nil {
				continue
			}
			tc.applyInsert(rowData, row)
		}
	}
}

func (cc *clientCache) ApplyTransactionUpdate(update *protocol.TransactionUpdate) {
	if update == nil {
		return
	}
	for _, qsUpdate := range update.QuerySets {
		for _, tableUpdate := range qsUpdate.Tables {
			tc, ok := cc.tables.Load(tableUpdate.TableName)
			if !ok {
				continue
			}
			for _, rows := range tableUpdate.Rows {
				switch r := rows.(type) {
				case *protocol.PersistentTableRows:
					// Check if this table supports PK-based update detection
					pkDef, hasPK := tc.def.(TableDefWithPK)

					if hasPK && r.Deletes != nil && r.Inserts != nil {
						// Build map of deleted rows by PK
						type deleteEntry struct {
							rowBytes []byte
							row      any
						}
						deletedByPK := map[any]deleteEntry{}
						for _, rowData := range r.Deletes.Rows() {
							reader := bsatn.NewReader(rowData)
							row, err := tc.def.DecodeRow(reader)
							if err != nil {
								continue
							}
							pk := pkDef.PrimaryKey(row)
							deletedByPK[pk] = deleteEntry{rowData, row}
						}
						// Process inserts, detecting updates
						for _, rowData := range r.Inserts.Rows() {
							reader := bsatn.NewReader(rowData)
							row, err := tc.def.DecodeRow(reader)
							if err != nil {
								continue
							}
							pk := pkDef.PrimaryKey(row)
							if old, isUpdate := deletedByPK[pk]; isUpdate {
								tc.applyUpdate(old.rowBytes, old.row, rowData, row)
								delete(deletedByPK, pk)
							} else {
								tc.applyInsert(rowData, row)
							}
						}
						// Remaining deletes are pure deletes
						for _, old := range deletedByPK {
							tc.applyDelete(old.rowBytes, old.row)
						}
					} else {
						// No PK — fall through to existing delete-then-insert logic
						if r.Deletes != nil {
							for _, rowData := range r.Deletes.Rows() {
								reader := bsatn.NewReader(rowData)
								row, err := tc.def.DecodeRow(reader)
								if err != nil {
									continue
								}
								tc.applyDelete(rowData, row)
							}
						}
						if r.Inserts != nil {
							for _, rowData := range r.Inserts.Rows() {
								reader := bsatn.NewReader(rowData)
								row, err := tc.def.DecodeRow(reader)
								if err != nil {
									continue
								}
								tc.applyInsert(rowData, row)
							}
						}
					}
				case *protocol.EventTableRows:
					if r.Events != nil {
						for _, rowData := range r.Events.Rows() {
							reader := bsatn.NewReader(rowData)
							row, err := tc.def.DecodeRow(reader)
							if err != nil {
								continue
							}
							tc.applyInsert(rowData, row)
						}
					}
				}
			}
		}
	}
}
