package realtime

import (
	"fmt"
	"sync"
	"time"
)

// 🌊 SPACETIMEDB REAL-TIME EVENT FRAMEWORK
// The beating heart of real-time multiplayer games!

// EventType represents the type of database event
type EventType int

const (
	EventInsert EventType = iota
	EventUpdate
	EventDelete
	EventBatch
	EventTransaction
)

func (et EventType) String() string {
	switch et {
	case EventInsert:
		return "INSERT"
	case EventUpdate:
		return "UPDATE"
	case EventDelete:
		return "DELETE"
	case EventBatch:
		return "BATCH"
	case EventTransaction:
		return "TRANSACTION"
	default:
		return "UNKNOWN"
	}
}

// TableEvent represents a real-time event on a table
type TableEvent struct {
	// Event metadata
	EventID   string    `json:"event_id"`
	Type      EventType `json:"type"`
	TableName string    `json:"table_name"`
	Timestamp time.Time `json:"timestamp"`

	// Entity data
	Entity    interface{} `json:"entity,omitempty"`     // New/current entity
	OldEntity interface{} `json:"old_entity,omitempty"` // Previous entity (for updates)

	// Change information
	PrimaryKey interface{}       `json:"primary_key"`
	Changes    map[string]Change `json:"changes,omitempty"` // Field-level changes

	// Metadata
	Source   string                 `json:"source,omitempty"`   // Source of the change
	UserID   interface{}            `json:"user_id,omitempty"`  // User who made the change
	Metadata map[string]interface{} `json:"metadata,omitempty"` // Additional context
}

// Change represents a field-level change
type Change struct {
	Field    string      `json:"field"`
	OldValue interface{} `json:"old_value"`
	NewValue interface{} `json:"new_value"`
}

// EventFilter defines criteria for filtering events
type EventFilter struct {
	Tables     []string               `json:"tables,omitempty"`
	EventTypes []EventType            `json:"event_types,omitempty"`
	Conditions map[string]interface{} `json:"conditions,omitempty"`
	Predicate  func(*TableEvent) bool `json:"-"` // Custom filter function
}

// Matches checks if an event matches this filter
func (f *EventFilter) Matches(event *TableEvent) bool {
	// Check table filter
	if len(f.Tables) > 0 {
		found := false
		for _, table := range f.Tables {
			if table == event.TableName {
				found = true
				break
			}
		}
		if !found {
			return false
		}
	}

	// Check event type filter
	if len(f.EventTypes) > 0 {
		found := false
		for _, eventType := range f.EventTypes {
			if eventType == event.Type {
				found = true
				break
			}
		}
		if !found {
			return false
		}
	}

	// Check custom predicate
	if f.Predicate != nil {
		return f.Predicate(event)
	}

	return true
}

// EventHandler defines a function that handles table events
type EventHandler func(*TableEvent)

// BatchEventHandler handles multiple events at once for performance
type BatchEventHandler func([]*TableEvent)

// ErrorHandler handles subscription errors
type ErrorHandler func(error)

// EventSubscription represents an active subscription to table events
type EventSubscription struct {
	ID           string            `json:"id"`
	Filter       *EventFilter      `json:"filter"`
	Handler      EventHandler      `json:"-"`
	BatchHandler BatchEventHandler `json:"-"`
	ErrorHandler ErrorHandler      `json:"-"`

	// Configuration
	BufferSize   int           `json:"buffer_size"`
	BatchSize    int           `json:"batch_size"`
	BatchTimeout time.Duration `json:"batch_timeout"`
	QueueSize    int           `json:"queue_size"`

	// Runtime state
	Active      bool      `json:"active"`
	CreatedAt   time.Time `json:"created_at"`
	LastEventAt time.Time `json:"last_event_at"`
	EventCount  int64     `json:"event_count"`
	ErrorCount  int64     `json:"error_count"`

	// Internal channels and control
	eventChan chan *TableEvent
	batchChan chan []*TableEvent
	stopChan  chan struct{}
	mu        sync.RWMutex
}

// EventBus manages the distribution of events to subscriptions
type EventBus struct {
	// Subscriptions management
	subscriptions map[string]*EventSubscription
	mu            sync.RWMutex

	// Event processing
	eventQueue chan *TableEvent
	workers    int
	stopChan   chan struct{}

	// Statistics
	totalEvents         int64
	totalSubscriptions  int64
	activeSubscriptions int64

	// Configuration
	maxQueueSize int
	workerCount  int
	bufferSize   int
	batchTimeout time.Duration
}

// NewEventBus creates a new high-performance event bus
func NewEventBus() *EventBus {
	bus := &EventBus{
		subscriptions: make(map[string]*EventSubscription),
		eventQueue:    make(chan *TableEvent, 10000), // High-capacity queue
		workers:       4,                             // Multi-core processing
		maxQueueSize:  10000,
		workerCount:   4,
		bufferSize:    1000,
		batchTimeout:  5 * time.Millisecond, // Ultra-low latency
		stopChan:      make(chan struct{}),
	}

	// Start event processing workers
	for i := 0; i < bus.workerCount; i++ {
		go bus.eventWorker()
	}

	return bus
}

// Subscribe creates a new subscription to table events
func (eb *EventBus) Subscribe(filter *EventFilter, handler EventHandler) (*EventSubscription, error) {
	eb.mu.Lock()
	defer eb.mu.Unlock()

	subscription := &EventSubscription{
		ID:           generateSubscriptionID(),
		Filter:       filter,
		Handler:      handler,
		BufferSize:   eb.bufferSize,
		BatchSize:    50,
		BatchTimeout: eb.batchTimeout,
		QueueSize:    1000,
		Active:       true,
		CreatedAt:    time.Now(),
		eventChan:    make(chan *TableEvent, 1000),
		stopChan:     make(chan struct{}),
	}

	// Start subscription processor
	go eb.subscriptionProcessor(subscription)

	eb.subscriptions[subscription.ID] = subscription
	eb.totalSubscriptions++
	eb.activeSubscriptions++

	return subscription, nil
}

// SubscribeBatch creates a subscription that handles events in batches
func (eb *EventBus) SubscribeBatch(filter *EventFilter, batchHandler BatchEventHandler, batchSize int, timeout time.Duration) (*EventSubscription, error) {
	eb.mu.Lock()
	defer eb.mu.Unlock()

	subscription := &EventSubscription{
		ID:           generateSubscriptionID(),
		Filter:       filter,
		BatchHandler: batchHandler,
		BufferSize:   eb.bufferSize,
		BatchSize:    batchSize,
		BatchTimeout: timeout,
		QueueSize:    1000,
		Active:       true,
		CreatedAt:    time.Now(),
		eventChan:    make(chan *TableEvent, 1000),
		batchChan:    make(chan []*TableEvent, 100),
		stopChan:     make(chan struct{}),
	}

	// Start batch processor
	go eb.batchProcessor(subscription)

	eb.subscriptions[subscription.ID] = subscription
	eb.totalSubscriptions++
	eb.activeSubscriptions++

	return subscription, nil
}

// Unsubscribe removes a subscription
func (eb *EventBus) Unsubscribe(subscriptionID string) error {
	eb.mu.Lock()
	defer eb.mu.Unlock()

	subscription, exists := eb.subscriptions[subscriptionID]
	if !exists {
		return fmt.Errorf("subscription %s not found", subscriptionID)
	}

	subscription.mu.Lock()
	subscription.Active = false
	close(subscription.stopChan)
	subscription.mu.Unlock()

	delete(eb.subscriptions, subscriptionID)
	eb.activeSubscriptions--

	return nil
}

// PublishEvent publishes an event to all matching subscriptions
func (eb *EventBus) PublishEvent(event *TableEvent) error {
	// Add timestamp if not set
	if event.Timestamp.IsZero() {
		event.Timestamp = time.Now()
	}

	// Add event ID if not set
	if event.EventID == "" {
		event.EventID = generateEventID()
	}

	select {
	case eb.eventQueue <- event:
		eb.totalEvents++
		return nil
	default:
		return fmt.Errorf("event queue full, dropping event %s", event.EventID)
	}
}

// PublishEvents publishes multiple events efficiently
func (eb *EventBus) PublishEvents(events []*TableEvent) error {
	for _, event := range events {
		if err := eb.PublishEvent(event); err != nil {
			return err
		}
	}
	return nil
}

// eventWorker processes events from the queue
func (eb *EventBus) eventWorker() {
	for {
		select {
		case event := <-eb.eventQueue:
			eb.distributeEvent(event)
		case <-eb.stopChan:
			return
		}
	}
}

// distributeEvent sends an event to all matching subscriptions
func (eb *EventBus) distributeEvent(event *TableEvent) {
	eb.mu.RLock()
	defer eb.mu.RUnlock()

	for _, subscription := range eb.subscriptions {
		if !subscription.Active {
			continue
		}

		if subscription.Filter != nil && !subscription.Filter.Matches(event) {
			continue
		}

		select {
		case subscription.eventChan <- event:
			subscription.EventCount++
			subscription.LastEventAt = time.Now()
		default:
			// Subscription queue full - could implement backpressure here
			subscription.ErrorCount++
			if subscription.ErrorHandler != nil {
				go subscription.ErrorHandler(fmt.Errorf("subscription queue full"))
			}
		}
	}
}

// subscriptionProcessor handles events for a single subscription
func (eb *EventBus) subscriptionProcessor(subscription *EventSubscription) {
	defer func() {
		if r := recover(); r != nil {
			if subscription.ErrorHandler != nil {
				subscription.ErrorHandler(fmt.Errorf("subscription processor panic: %v", r))
			}
		}
	}()

	for {
		select {
		case event := <-subscription.eventChan:
			if subscription.Handler != nil {
				// Handle single event
				func() {
					defer func() {
						if r := recover(); r != nil {
							subscription.ErrorCount++
							if subscription.ErrorHandler != nil {
								subscription.ErrorHandler(fmt.Errorf("event handler panic: %v", r))
							}
						}
					}()
					subscription.Handler(event)
				}()
			}
		case <-subscription.stopChan:
			return
		}
	}
}

// batchProcessor handles events in batches for a subscription
func (eb *EventBus) batchProcessor(subscription *EventSubscription) {
	defer func() {
		if r := recover(); r != nil {
			if subscription.ErrorHandler != nil {
				subscription.ErrorHandler(fmt.Errorf("batch processor panic: %v", r))
			}
		}
	}()

	ticker := time.NewTicker(subscription.BatchTimeout)
	defer ticker.Stop()

	var batch []*TableEvent

	for {
		select {
		case event := <-subscription.eventChan:
			batch = append(batch, event)

			// Send batch if it reaches the target size
			if len(batch) >= subscription.BatchSize {
				eb.sendBatch(subscription, batch)
				batch = nil
			}

		case <-ticker.C:
			// Send partial batch on timeout
			if len(batch) > 0 {
				eb.sendBatch(subscription, batch)
				batch = nil
			}

		case <-subscription.stopChan:
			// Send final batch before stopping
			if len(batch) > 0 {
				eb.sendBatch(subscription, batch)
			}
			return
		}
	}
}

// sendBatch sends a batch of events to the subscription handler
func (eb *EventBus) sendBatch(subscription *EventSubscription, batch []*TableEvent) {
	if subscription.BatchHandler == nil {
		return
	}

	func() {
		defer func() {
			if r := recover(); r != nil {
				subscription.ErrorCount++
				if subscription.ErrorHandler != nil {
					subscription.ErrorHandler(fmt.Errorf("batch handler panic: %v", r))
				}
			}
		}()
		subscription.BatchHandler(batch)
	}()
}

// Stats returns current event bus statistics
func (eb *EventBus) Stats() EventBusStats {
	eb.mu.RLock()
	defer eb.mu.RUnlock()

	return EventBusStats{
		TotalEvents:         eb.totalEvents,
		TotalSubscriptions:  eb.totalSubscriptions,
		ActiveSubscriptions: eb.activeSubscriptions,
		QueueLength:         len(eb.eventQueue),
		QueueCapacity:       cap(eb.eventQueue),
		WorkerCount:         eb.workerCount,
	}
}

// EventBusStats contains event bus performance statistics
type EventBusStats struct {
	TotalEvents         int64 `json:"total_events"`
	TotalSubscriptions  int64 `json:"total_subscriptions"`
	ActiveSubscriptions int64 `json:"active_subscriptions"`
	QueueLength         int   `json:"queue_length"`
	QueueCapacity       int   `json:"queue_capacity"`
	WorkerCount         int   `json:"worker_count"`
}

// Shutdown gracefully stops the event bus
func (eb *EventBus) Shutdown() {
	eb.mu.Lock()
	defer eb.mu.Unlock()

	// Stop all subscriptions
	for _, subscription := range eb.subscriptions {
		subscription.mu.Lock()
		subscription.Active = false
		close(subscription.stopChan)
		subscription.mu.Unlock()
	}

	// Stop workers
	close(eb.stopChan)
}

// Utility functions

var (
	eventIDCounter        int64
	subscriptionIDCounter int64
	idMutex               sync.Mutex
)

func generateEventID() string {
	idMutex.Lock()
	defer idMutex.Unlock()
	eventIDCounter++
	return fmt.Sprintf("evt_%d_%d", time.Now().Unix(), eventIDCounter)
}

func generateSubscriptionID() string {
	idMutex.Lock()
	defer idMutex.Unlock()
	subscriptionIDCounter++
	return fmt.Sprintf("sub_%d_%d", time.Now().Unix(), subscriptionIDCounter)
}

// Helper functions for creating common event filters

// TableFilter creates a filter for specific tables
func TableFilter(tables ...string) *EventFilter {
	return &EventFilter{
		Tables: tables,
	}
}

// EventTypeFilter creates a filter for specific event types
func EventTypeFilter(eventTypes ...EventType) *EventFilter {
	return &EventFilter{
		EventTypes: eventTypes,
	}
}

// PredicateFilter creates a filter with a custom predicate function
func PredicateFilter(predicate func(*TableEvent) bool) *EventFilter {
	return &EventFilter{
		Predicate: predicate,
	}
}

// CombinedFilter combines multiple filter criteria
func CombinedFilter(tables []string, eventTypes []EventType, predicate func(*TableEvent) bool) *EventFilter {
	return &EventFilter{
		Tables:     tables,
		EventTypes: eventTypes,
		Predicate:  predicate,
	}
}
