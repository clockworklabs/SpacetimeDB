package cache

import (
	"sync/atomic"

	"github.com/puzpuzpuz/xsync/v3"
)

// TableCache stores rows for a single table.
type TableCache interface {
	Count() int
	Iter(fn func(row any) bool)
	OnInsert(cb InsertCallback) CallbackID
	OnDelete(cb DeleteCallback) CallbackID
	OnUpdate(cb UpdateCallback) CallbackID
	RemoveCallback(id CallbackID)
}

// newTableCache creates a new table cache.
func newTableCache(def TableDef) *tableCache {
	return &tableCache{
		def:             def,
		rows:            xsync.NewMapOf[string, any](),
		insertCallbacks: xsync.NewMapOf[CallbackID, InsertCallback](),
		deleteCallbacks: xsync.NewMapOf[CallbackID, DeleteCallback](),
		updateCallbacks: xsync.NewMapOf[CallbackID, UpdateCallback](),
	}
}

type tableCache struct {
	def             TableDef
	rows            *xsync.MapOf[string, any]
	insertCallbacks *xsync.MapOf[CallbackID, InsertCallback]
	deleteCallbacks *xsync.MapOf[CallbackID, DeleteCallback]
	updateCallbacks *xsync.MapOf[CallbackID, UpdateCallback]
	nextCallbackID  atomic.Uint64
}

func (tc *tableCache) Count() int {
	return tc.rows.Size()
}

func (tc *tableCache) Iter(fn func(row any) bool) {
	tc.rows.Range(func(key string, value any) bool {
		return fn(value)
	})
}

func (tc *tableCache) OnInsert(cb InsertCallback) CallbackID {
	id := CallbackID(tc.nextCallbackID.Add(1))
	tc.insertCallbacks.Store(id, cb)
	return id
}

func (tc *tableCache) OnDelete(cb DeleteCallback) CallbackID {
	id := CallbackID(tc.nextCallbackID.Add(1))
	tc.deleteCallbacks.Store(id, cb)
	return id
}

func (tc *tableCache) OnUpdate(cb UpdateCallback) CallbackID {
	id := CallbackID(tc.nextCallbackID.Add(1))
	tc.updateCallbacks.Store(id, cb)
	return id
}

func (tc *tableCache) RemoveCallback(id CallbackID) {
	tc.insertCallbacks.Delete(id)
	tc.deleteCallbacks.Delete(id)
	tc.updateCallbacks.Delete(id)
}

// applyInsert stores a row and fires insert callbacks.
func (tc *tableCache) applyInsert(rowBytes []byte, row any) {
	key := string(rowBytes)
	tc.rows.Store(key, row)
	tc.insertCallbacks.Range(func(_ CallbackID, cb InsertCallback) bool {
		cb(row)
		return true
	})
}

// applyDelete removes a row and fires delete callbacks.
func (tc *tableCache) applyDelete(rowBytes []byte, row any) {
	key := string(rowBytes)
	tc.rows.Delete(key)
	tc.deleteCallbacks.Range(func(_ CallbackID, cb DeleteCallback) bool {
		cb(row)
		return true
	})
}

// applyUpdate removes old row, stores new row, and fires update callbacks.
func (tc *tableCache) applyUpdate(oldRowBytes []byte, oldRow any, newRowBytes []byte, newRow any) {
	tc.rows.Delete(string(oldRowBytes))
	tc.rows.Store(string(newRowBytes), newRow)
	tc.updateCallbacks.Range(func(_ CallbackID, cb UpdateCallback) bool {
		cb(oldRow, newRow)
		return true
	})
}
