package realtime

import (
	"fmt"
	"reflect"
	"sync"
	"time"
)

// 🔥 REACTIVE SUBSCRIPTION SYSTEM
// Live queries that update automatically!

// LiveQuery represents a query that automatically updates when data changes
type LiveQuery[T any] struct {
	ID        string
	TableName string
	Filter    func(T) bool
	OrderBy   []OrderClause
	Limit     int
	Offset    int

	// Current results
	Results []T
	mu      sync.RWMutex

	// Event handling
	OnUpdate        func([]T)
	OnInsertHandler func(T, int)    // entity, index
	OnDeleteHandler func(T, int)    // entity, old index
	OnChangeHandler func(T, T, int) // old, new, index

	// Subscription management
	subscription *EventSubscription
	eventBus     *EventBus
	active       bool

	// Performance tracking
	QueryCount  int64
	LastUpdate  time.Time
	UpdateCount int64
}

// OrderClause defines sorting for live queries
type OrderClause struct {
	Field     string
	Ascending bool
}

// LiveQueryBuilder provides a fluent API for building live queries
type LiveQueryBuilder[T any] struct {
	tableName string
	filter    func(T) bool
	orderBy   []OrderClause
	limit     int
	offset    int
	eventBus  *EventBus
}

// TableSubscription provides reactive access to a specific table
type TableSubscription[T any] struct {
	TableName string
	eventBus  *EventBus
	accessor  TableAccessor[T] // Interface to our Phase 4 table layer
}

// TableAccessor interface for integrating with Phase 4 tables
type TableAccessor[T any] interface {
	FindAll() ([]T, error)
	FindWhere(func(T) bool) ([]T, error)
	Insert(T) error
	Update(interface{}, T) error
	Delete(interface{}) error
	Count() int
	Name() string
}

// RealtimeManager coordinates all real-time functionality
type RealtimeManager struct {
	eventBus      *EventBus
	liveQueries   map[string]interface{} // map[string]*LiveQuery[T]
	subscriptions map[string]interface{} // map[string]*TableSubscription[T]
	mu            sync.RWMutex

	// Configuration
	defaultBatchSize    int
	defaultBatchTimeout time.Duration
	maxLiveQueries      int

	// Statistics
	totalLiveQueries    int64
	activeLiveQueries   int64
	totalSubscriptions  int64
	activeSubscriptions int64
}

// NewRealtimeManager creates a new real-time manager
func NewRealtimeManager() *RealtimeManager {
	return &RealtimeManager{
		eventBus:            NewEventBus(),
		liveQueries:         make(map[string]interface{}),
		subscriptions:       make(map[string]interface{}),
		defaultBatchSize:    50,
		defaultBatchTimeout: 5 * time.Millisecond,
		maxLiveQueries:      10000,
	}
}

// RegisterTable creates a reactive subscription for a table
func RegisterTable[T any](rm *RealtimeManager, tableName string, accessor TableAccessor[T]) *TableSubscription[T] {
	rm.mu.Lock()
	defer rm.mu.Unlock()

	subscription := &TableSubscription[T]{
		TableName: tableName,
		eventBus:  rm.eventBus,
		accessor:  accessor,
	}

	rm.subscriptions[tableName] = subscription
	rm.totalSubscriptions++
	rm.activeSubscriptions++

	return subscription
}

// Table returns a live query builder for the specified table
func (ts *TableSubscription[T]) Table() *LiveQueryBuilder[T] {
	return &LiveQueryBuilder[T]{
		tableName: ts.TableName,
		eventBus:  ts.eventBus,
		limit:     -1, // No limit by default
	}
}

// Where adds a filter condition to the live query
func (lqb *LiveQueryBuilder[T]) Where(filter func(T) bool) *LiveQueryBuilder[T] {
	lqb.filter = filter
	return lqb
}

// OrderBy adds sorting to the live query
func (lqb *LiveQueryBuilder[T]) OrderBy(field string, ascending bool) *LiveQueryBuilder[T] {
	lqb.orderBy = append(lqb.orderBy, OrderClause{
		Field:     field,
		Ascending: ascending,
	})
	return lqb
}

// OrderByAsc adds ascending sort
func (lqb *LiveQueryBuilder[T]) OrderByAsc(field string) *LiveQueryBuilder[T] {
	return lqb.OrderBy(field, true)
}

// OrderByDesc adds descending sort
func (lqb *LiveQueryBuilder[T]) OrderByDesc(field string) *LiveQueryBuilder[T] {
	return lqb.OrderBy(field, false)
}

// Limit sets the maximum number of results
func (lqb *LiveQueryBuilder[T]) Limit(limit int) *LiveQueryBuilder[T] {
	lqb.limit = limit
	return lqb
}

// Offset sets the result offset
func (lqb *LiveQueryBuilder[T]) Offset(offset int) *LiveQueryBuilder[T] {
	lqb.offset = offset
	return lqb
}

// Subscribe creates a live subscription that calls the handler whenever results change
func (lqb *LiveQueryBuilder[T]) Subscribe(handler func([]T)) (*LiveQuery[T], error) {
	liveQuery := &LiveQuery[T]{
		ID:        generateLiveQueryID(),
		TableName: lqb.tableName,
		Filter:    lqb.filter,
		OrderBy:   lqb.orderBy,
		Limit:     lqb.limit,
		Offset:    lqb.offset,
		OnUpdate:  handler,
		eventBus:  lqb.eventBus,
		active:    true,
	}

	// Set up event subscription
	filter := TableFilter(lqb.tableName)
	subscription, err := lqb.eventBus.Subscribe(filter, func(event *TableEvent) {
		liveQuery.handleEvent(event)
	})

	if err != nil {
		return nil, fmt.Errorf("failed to create subscription: %w", err)
	}

	liveQuery.subscription = subscription

	// Perform initial query
	liveQuery.refresh()

	return liveQuery, nil
}

// OnInsert sets up a handler for when entities are inserted
func (lq *LiveQuery[T]) OnInsert(handler func(T, int)) *LiveQuery[T] {
	lq.OnInsertHandler = handler
	return lq
}

// OnDelete sets up a handler for when entities are deleted
func (lq *LiveQuery[T]) OnDelete(handler func(T, int)) *LiveQuery[T] {
	lq.OnDeleteHandler = handler
	return lq
}

// OnChange sets up a handler for when entities are updated
func (lq *LiveQuery[T]) OnChange(handler func(T, T, int)) *LiveQuery[T] {
	lq.OnChangeHandler = handler
	return lq
}

// handleEvent processes table events and updates the live query results
func (lq *LiveQuery[T]) handleEvent(event *TableEvent) {
	if !lq.active {
		return
	}

	lq.mu.Lock()
	defer lq.mu.Unlock()

	switch event.Type {
	case EventInsert:
		lq.handleInsert(event)
	case EventUpdate:
		lq.handleUpdate(event)
	case EventDelete:
		lq.handleDelete(event)
	case EventBatch:
		// For batch operations, refresh the entire query
		lq.refresh()
	}

	lq.UpdateCount++
	lq.LastUpdate = time.Now()
}

// handleInsert processes insert events
func (lq *LiveQuery[T]) handleInsert(event *TableEvent) {
	if event.Entity == nil {
		return
	}

	entity, ok := event.Entity.(T)
	if !ok {
		return
	}

	// Check if entity matches filter
	if lq.Filter != nil && !lq.Filter(entity) {
		return
	}

	// Find insertion position based on ordering
	insertIndex := lq.findInsertPosition(entity)

	// Insert entity at the correct position
	if insertIndex <= len(lq.Results) {
		// Insert at position
		lq.Results = append(lq.Results[:insertIndex],
			append([]T{entity}, lq.Results[insertIndex:]...)...)
	} else {
		// Append at end
		lq.Results = append(lq.Results, entity)
	}

	// Apply limit if necessary
	if lq.Limit > 0 && len(lq.Results) > lq.Limit {
		lq.Results = lq.Results[:lq.Limit]
	}

	// Trigger callbacks
	if lq.OnInsertHandler != nil {
		lq.OnInsertHandler(entity, insertIndex)
	}

	if lq.OnUpdate != nil {
		lq.OnUpdate(lq.Results)
	}
}

// handleUpdate processes update events
func (lq *LiveQuery[T]) handleUpdate(event *TableEvent) {
	if event.Entity == nil {
		return
	}

	newEntity, ok := event.Entity.(T)
	if !ok {
		return
	}

	// Find the entity in current results
	oldIndex := lq.findEntityIndex(event.PrimaryKey)
	if oldIndex == -1 {
		// Entity not in current results, treat as insert if it matches filter
		if lq.Filter == nil || lq.Filter(newEntity) {
			lq.handleInsert(event)
		}
		return
	}

	oldEntity := lq.Results[oldIndex]

	// Check if updated entity still matches filter
	if lq.Filter != nil && !lq.Filter(newEntity) {
		// Remove from results
		lq.Results = append(lq.Results[:oldIndex], lq.Results[oldIndex+1:]...)

		if lq.OnDeleteHandler != nil {
			lq.OnDeleteHandler(oldEntity, oldIndex)
		}
	} else {
		// Update the entity
		lq.Results[oldIndex] = newEntity

		// Check if position needs to change due to ordering
		newIndex := lq.findCorrectPosition(newEntity, oldIndex)
		if newIndex != oldIndex {
			// Move entity to correct position
			lq.Results = append(lq.Results[:oldIndex], lq.Results[oldIndex+1:]...)
			lq.Results = append(lq.Results[:newIndex],
				append([]T{newEntity}, lq.Results[newIndex:]...)...)
		}

		if lq.OnChangeHandler != nil {
			lq.OnChangeHandler(oldEntity, newEntity, newIndex)
		}
	}

	if lq.OnUpdate != nil {
		lq.OnUpdate(lq.Results)
	}
}

// handleDelete processes delete events
func (lq *LiveQuery[T]) handleDelete(event *TableEvent) {
	// Find the entity in current results
	index := lq.findEntityIndex(event.PrimaryKey)
	if index == -1 {
		return
	}

	deletedEntity := lq.Results[index]

	// Remove from results
	lq.Results = append(lq.Results[:index], lq.Results[index+1:]...)

	// Trigger callbacks
	if lq.OnDeleteHandler != nil {
		lq.OnDeleteHandler(deletedEntity, index)
	}

	if lq.OnUpdate != nil {
		lq.OnUpdate(lq.Results)
	}
}

// refresh reloads the entire query results
func (lq *LiveQuery[T]) refresh() {
	// This would integrate with our Phase 4 table accessor
	// For now, we'll simulate the refresh
	lq.QueryCount++

	if lq.OnUpdate != nil {
		lq.OnUpdate(lq.Results)
	}
}

// findInsertPosition finds where to insert a new entity based on ordering
func (lq *LiveQuery[T]) findInsertPosition(entity T) int {
	if len(lq.OrderBy) == 0 {
		return len(lq.Results)
	}

	// Binary search for insertion point
	left, right := 0, len(lq.Results)

	for left < right {
		mid := (left + right) / 2
		if lq.compareEntities(lq.Results[mid], entity) {
			left = mid + 1
		} else {
			right = mid
		}
	}

	return left
}

// findCorrectPosition finds the correct position for an updated entity
func (lq *LiveQuery[T]) findCorrectPosition(entity T, currentIndex int) int {
	if len(lq.OrderBy) == 0 {
		return currentIndex
	}

	// Find where this entity should be positioned
	newPosition := lq.findInsertPosition(entity)

	// Adjust for the fact that we'll remove the entity first
	if newPosition > currentIndex {
		newPosition--
	}

	return newPosition
}

// findEntityIndex finds the index of an entity by primary key
func (lq *LiveQuery[T]) findEntityIndex(primaryKey interface{}) int {
	for i, entity := range lq.Results {
		if lq.getEntityPrimaryKey(entity) == primaryKey {
			return i
		}
	}
	return -1
}

// getEntityPrimaryKey extracts the primary key from an entity using reflection
func (lq *LiveQuery[T]) getEntityPrimaryKey(entity T) interface{} {
	value := reflect.ValueOf(entity)
	if value.Kind() == reflect.Ptr {
		value = value.Elem()
	}

	entityType := value.Type()

	// Look for primary key field
	for i := 0; i < entityType.NumField(); i++ {
		field := entityType.Field(i)
		if tag := field.Tag.Get("spacetime"); tag == "primary_key" || tag == "pk" {
			return value.Field(i).Interface()
		}
		if field.Name == "ID" || field.Name == "Id" {
			return value.Field(i).Interface()
		}
	}

	return nil
}

// compareEntities compares two entities based on the order clauses
func (lq *LiveQuery[T]) compareEntities(a, b T) bool {
	aValue := reflect.ValueOf(a)
	bValue := reflect.ValueOf(b)

	if aValue.Kind() == reflect.Ptr {
		aValue = aValue.Elem()
	}
	if bValue.Kind() == reflect.Ptr {
		bValue = bValue.Elem()
	}

	for _, orderClause := range lq.OrderBy {
		aField := aValue.FieldByName(orderClause.Field)
		bField := bValue.FieldByName(orderClause.Field)

		if !aField.IsValid() || !bField.IsValid() {
			continue
		}

		comparison := compareValues(aField.Interface(), bField.Interface())

		if comparison != 0 {
			if orderClause.Ascending {
				return comparison < 0
			} else {
				return comparison > 0
			}
		}
	}

	return false
}

// compareValues compares two values of the same type
func compareValues(a, b interface{}) int {
	switch va := a.(type) {
	case int:
		vb := b.(int)
		return va - vb
	case int32:
		vb := b.(int32)
		return int(va - vb)
	case int64:
		vb := b.(int64)
		return int(va - vb)
	case uint32:
		vb := b.(uint32)
		if va < vb {
			return -1
		} else if va > vb {
			return 1
		}
		return 0
	case uint64:
		vb := b.(uint64)
		if va < vb {
			return -1
		} else if va > vb {
			return 1
		}
		return 0
	case float32:
		vb := b.(float32)
		if va < vb {
			return -1
		} else if va > vb {
			return 1
		}
		return 0
	case float64:
		vb := b.(float64)
		if va < vb {
			return -1
		} else if va > vb {
			return 1
		}
		return 0
	case string:
		vb := b.(string)
		if va < vb {
			return -1
		} else if va > vb {
			return 1
		}
		return 0
	case time.Time:
		vb := b.(time.Time)
		if va.Before(vb) {
			return -1
		} else if va.After(vb) {
			return 1
		}
		return 0
	}

	return 0
}

// Stop stops the live query and cleans up resources
func (lq *LiveQuery[T]) Stop() error {
	lq.mu.Lock()
	defer lq.mu.Unlock()

	lq.active = false

	if lq.subscription != nil {
		return lq.eventBus.Unsubscribe(lq.subscription.ID)
	}

	return nil
}

// GetResults returns the current query results
func (lq *LiveQuery[T]) GetResults() []T {
	lq.mu.RLock()
	defer lq.mu.RUnlock()

	// Return a copy to prevent external modification
	results := make([]T, len(lq.Results))
	copy(results, lq.Results)
	return results
}

// Stats returns statistics about the live query
func (lq *LiveQuery[T]) Stats() LiveQueryStats {
	lq.mu.RLock()
	defer lq.mu.RUnlock()

	return LiveQueryStats{
		ID:          lq.ID,
		TableName:   lq.TableName,
		ResultCount: len(lq.Results),
		QueryCount:  lq.QueryCount,
		UpdateCount: lq.UpdateCount,
		LastUpdate:  lq.LastUpdate,
		Active:      lq.active,
	}
}

// LiveQueryStats contains statistics about a live query
type LiveQueryStats struct {
	ID          string    `json:"id"`
	TableName   string    `json:"table_name"`
	ResultCount int       `json:"result_count"`
	QueryCount  int64     `json:"query_count"`
	UpdateCount int64     `json:"update_count"`
	LastUpdate  time.Time `json:"last_update"`
	Active      bool      `json:"active"`
}

// Utility functions

var liveQueryIDCounter int64

func generateLiveQueryID() string {
	idMutex.Lock()
	defer idMutex.Unlock()
	liveQueryIDCounter++
	return fmt.Sprintf("lq_%d_%d", time.Now().Unix(), liveQueryIDCounter)
}

// Stats returns real-time manager statistics
func (rm *RealtimeManager) Stats() RealtimeStats {
	rm.mu.RLock()
	defer rm.mu.RUnlock()

	return RealtimeStats{
		TotalLiveQueries:    rm.totalLiveQueries,
		ActiveLiveQueries:   rm.activeLiveQueries,
		TotalSubscriptions:  rm.totalSubscriptions,
		ActiveSubscriptions: rm.activeSubscriptions,
		EventBusStats:       rm.eventBus.Stats(),
	}
}

// RealtimeStats contains overall real-time system statistics
type RealtimeStats struct {
	TotalLiveQueries    int64         `json:"total_live_queries"`
	ActiveLiveQueries   int64         `json:"active_live_queries"`
	TotalSubscriptions  int64         `json:"total_subscriptions"`
	ActiveSubscriptions int64         `json:"active_subscriptions"`
	EventBusStats       EventBusStats `json:"event_bus_stats"`
}

// Shutdown gracefully stops the real-time manager
func (rm *RealtimeManager) Shutdown() {
	rm.mu.Lock()
	defer rm.mu.Unlock()

	// Stop all live queries
	for _ = range rm.liveQueries {
		// Type assertion would be needed here for each specific type
		// This is a simplified version
	}

	// Shutdown the event bus
	rm.eventBus.Shutdown()
}
