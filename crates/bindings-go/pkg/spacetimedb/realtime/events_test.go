package realtime

import (
	"sync"
	"testing"
	"time"
)

// Test entity types
type TestPlayer struct {
	ID     uint32 `spacetime:"primary_key"`
	Name   string
	Score  uint32
	Active bool
}

type TestGame struct {
	ID       uint32 `spacetime:"primary_key"`
	Name     string
	PlayerID uint32
	Status   string
}

func TestEventBusCreation(t *testing.T) {
	bus := NewEventBus()
	if bus == nil {
		t.Fatal("EventBus creation failed")
	}

	stats := bus.Stats()
	if stats.TotalEvents != 0 {
		t.Errorf("Expected 0 total events, got %d", stats.TotalEvents)
	}

	if stats.ActiveSubscriptions != 0 {
		t.Errorf("Expected 0 active subscriptions, got %d", stats.ActiveSubscriptions)
	}

	bus.Shutdown()
}

func TestEventSubscription(t *testing.T) {
	bus := NewEventBus()
	defer bus.Shutdown()

	// Set up subscription
	var receivedEvents []*TableEvent
	var eventMutex sync.Mutex

	filter := TableFilter("players")
	subscription, err := bus.Subscribe(filter, func(event *TableEvent) {
		eventMutex.Lock()
		receivedEvents = append(receivedEvents, event)
		eventMutex.Unlock()
	})

	if err != nil {
		t.Fatalf("Failed to create subscription: %v", err)
	}

	if subscription.ID == "" {
		t.Error("Subscription should have an ID")
	}

	// Publish test event
	event := &TableEvent{
		Type:      EventInsert,
		TableName: "players",
		Entity: TestPlayer{
			ID:     1,
			Name:   "Alice",
			Score:  1500,
			Active: true,
		},
		PrimaryKey: uint32(1),
	}

	err = bus.PublishEvent(event)
	if err != nil {
		t.Errorf("Failed to publish event: %v", err)
	}

	// Wait for event processing
	time.Sleep(10 * time.Millisecond)

	eventMutex.Lock()
	eventCount := len(receivedEvents)
	eventMutex.Unlock()

	if eventCount != 1 {
		t.Errorf("Expected 1 event, got %d", eventCount)
	}

	// Verify event details
	if eventCount > 0 {
		receivedEvent := receivedEvents[0]
		if receivedEvent.Type != EventInsert {
			t.Errorf("Expected EventInsert, got %v", receivedEvent.Type)
		}

		if receivedEvent.TableName != "players" {
			t.Errorf("Expected table 'players', got '%s'", receivedEvent.TableName)
		}
	}
}

func TestEventFiltering(t *testing.T) {
	bus := NewEventBus()
	defer bus.Shutdown()

	// Create subscription for players table only
	var playerEvents []*TableEvent
	var playerMutex sync.Mutex

	playerFilter := TableFilter("players")
	_, err := bus.Subscribe(playerFilter, func(event *TableEvent) {
		playerMutex.Lock()
		playerEvents = append(playerEvents, event)
		playerMutex.Unlock()
	})

	if err != nil {
		t.Fatalf("Failed to create player subscription: %v", err)
	}

	// Create subscription for game events only
	var gameEvents []*TableEvent
	var gameMutex sync.Mutex

	gameFilter := TableFilter("games")
	_, err = bus.Subscribe(gameFilter, func(event *TableEvent) {
		gameMutex.Lock()
		gameEvents = append(gameEvents, event)
		gameMutex.Unlock()
	})

	if err != nil {
		t.Fatalf("Failed to create game subscription: %v", err)
	}

	// Publish player event
	playerEvent := &TableEvent{
		Type:      EventInsert,
		TableName: "players",
		Entity:    TestPlayer{ID: 1, Name: "Alice"},
	}

	// Publish game event
	gameEvent := &TableEvent{
		Type:      EventInsert,
		TableName: "games",
		Entity:    TestGame{ID: 1, Name: "Test Game"},
	}

	bus.PublishEvent(playerEvent)
	bus.PublishEvent(gameEvent)

	// Wait for processing
	time.Sleep(10 * time.Millisecond)

	// Verify filtering
	playerMutex.Lock()
	playerEventCount := len(playerEvents)
	playerMutex.Unlock()

	gameMutex.Lock()
	gameEventCount := len(gameEvents)
	gameMutex.Unlock()

	if playerEventCount != 1 {
		t.Errorf("Expected 1 player event, got %d", playerEventCount)
	}

	if gameEventCount != 1 {
		t.Errorf("Expected 1 game event, got %d", gameEventCount)
	}
}

func TestBatchSubscription(t *testing.T) {
	bus := NewEventBus()
	defer bus.Shutdown()

	// Set up batch subscription
	var batches [][]*TableEvent
	var batchMutex sync.Mutex

	filter := TableFilter("players")
	batchSize := 3
	timeout := 50 * time.Millisecond

	_, err := bus.SubscribeBatch(filter, func(events []*TableEvent) {
		batchMutex.Lock()
		batches = append(batches, events)
		batchMutex.Unlock()
	}, batchSize, timeout)

	if err != nil {
		t.Fatalf("Failed to create batch subscription: %v", err)
	}

	// Publish events to trigger batch
	for i := 0; i < 5; i++ {
		event := &TableEvent{
			Type:      EventInsert,
			TableName: "players",
			Entity: TestPlayer{
				ID:   uint32(i + 1),
				Name: "Player",
			},
		}
		bus.PublishEvent(event)
	}

	// Wait for batch processing
	time.Sleep(100 * time.Millisecond)

	batchMutex.Lock()
	batchCount := len(batches)
	batchMutex.Unlock()

	if batchCount < 1 {
		t.Errorf("Expected at least 1 batch, got %d", batchCount)
	}

	// Verify first batch size
	if batchCount > 0 {
		firstBatchSize := len(batches[0])
		if firstBatchSize != batchSize {
			t.Errorf("Expected first batch size %d, got %d", batchSize, firstBatchSize)
		}
	}
}

func TestEventTypeFiltering(t *testing.T) {
	bus := NewEventBus()
	defer bus.Shutdown()

	// Subscribe to insert events only
	var insertEvents []*TableEvent
	var insertMutex sync.Mutex

	insertFilter := EventTypeFilter(EventInsert)
	_, err := bus.Subscribe(insertFilter, func(event *TableEvent) {
		insertMutex.Lock()
		insertEvents = append(insertEvents, event)
		insertMutex.Unlock()
	})

	if err != nil {
		t.Fatalf("Failed to create insert subscription: %v", err)
	}

	// Subscribe to update events only
	var updateEvents []*TableEvent
	var updateMutex sync.Mutex

	updateFilter := EventTypeFilter(EventUpdate)
	_, err = bus.Subscribe(updateFilter, func(event *TableEvent) {
		updateMutex.Lock()
		updateEvents = append(updateEvents, event)
		updateMutex.Unlock()
	})

	if err != nil {
		t.Fatalf("Failed to create update subscription: %v", err)
	}

	// Publish different event types
	insertEvent := &TableEvent{
		Type:      EventInsert,
		TableName: "players",
		Entity:    TestPlayer{ID: 1, Name: "Alice"},
	}

	updateEvent := &TableEvent{
		Type:      EventUpdate,
		TableName: "players",
		Entity:    TestPlayer{ID: 1, Name: "Alice Updated"},
	}

	deleteEvent := &TableEvent{
		Type:       EventDelete,
		TableName:  "players",
		PrimaryKey: uint32(1),
	}

	bus.PublishEvent(insertEvent)
	bus.PublishEvent(updateEvent)
	bus.PublishEvent(deleteEvent)

	// Wait for processing
	time.Sleep(10 * time.Millisecond)

	// Verify type filtering
	insertMutex.Lock()
	insertCount := len(insertEvents)
	insertMutex.Unlock()

	updateMutex.Lock()
	updateCount := len(updateEvents)
	updateMutex.Unlock()

	if insertCount != 1 {
		t.Errorf("Expected 1 insert event, got %d", insertCount)
	}

	if updateCount != 1 {
		t.Errorf("Expected 1 update event, got %d", updateCount)
	}
}

func TestUnsubscribe(t *testing.T) {
	bus := NewEventBus()
	defer bus.Shutdown()

	// Create subscription
	var eventCount int
	var eventMutex sync.Mutex

	filter := TableFilter("players")
	subscription, err := bus.Subscribe(filter, func(event *TableEvent) {
		eventMutex.Lock()
		eventCount++
		eventMutex.Unlock()
	})

	if err != nil {
		t.Fatalf("Failed to create subscription: %v", err)
	}

	// Publish event (should be received)
	event1 := &TableEvent{
		Type:      EventInsert,
		TableName: "players",
		Entity:    TestPlayer{ID: 1, Name: "Alice"},
	}

	bus.PublishEvent(event1)
	time.Sleep(10 * time.Millisecond)

	// Unsubscribe
	err = bus.Unsubscribe(subscription.ID)
	if err != nil {
		t.Errorf("Failed to unsubscribe: %v", err)
	}

	// Publish another event (should not be received)
	event2 := &TableEvent{
		Type:      EventInsert,
		TableName: "players",
		Entity:    TestPlayer{ID: 2, Name: "Bob"},
	}

	bus.PublishEvent(event2)
	time.Sleep(10 * time.Millisecond)

	eventMutex.Lock()
	finalCount := eventCount
	eventMutex.Unlock()

	if finalCount != 1 {
		t.Errorf("Expected 1 event after unsubscribe, got %d", finalCount)
	}
}

func TestPredicateFilter(t *testing.T) {
	bus := NewEventBus()
	defer bus.Shutdown()

	// Subscribe to events for active players only
	var activePlayerEvents []*TableEvent
	var activeMutex sync.Mutex

	activeFilter := PredicateFilter(func(event *TableEvent) bool {
		if player, ok := event.Entity.(TestPlayer); ok {
			return player.Active
		}
		return false
	})

	_, err := bus.Subscribe(activeFilter, func(event *TableEvent) {
		activeMutex.Lock()
		activePlayerEvents = append(activePlayerEvents, event)
		activeMutex.Unlock()
	})

	if err != nil {
		t.Fatalf("Failed to create predicate subscription: %v", err)
	}

	// Publish events for active and inactive players
	activePlayer := &TableEvent{
		Type:      EventInsert,
		TableName: "players",
		Entity:    TestPlayer{ID: 1, Name: "Alice", Active: true},
	}

	inactivePlayer := &TableEvent{
		Type:      EventInsert,
		TableName: "players",
		Entity:    TestPlayer{ID: 2, Name: "Bob", Active: false},
	}

	bus.PublishEvent(activePlayer)
	bus.PublishEvent(inactivePlayer)

	// Wait for processing
	time.Sleep(10 * time.Millisecond)

	// Verify predicate filtering
	activeMutex.Lock()
	activeCount := len(activePlayerEvents)
	activeMutex.Unlock()

	if activeCount != 1 {
		t.Errorf("Expected 1 active player event, got %d", activeCount)
	}
}

func TestEventBusPerformance(t *testing.T) {
	bus := NewEventBus()
	defer bus.Shutdown()

	// Set up subscription
	var eventCount int64
	var eventMutex sync.Mutex

	filter := TableFilter("players")
	_, err := bus.Subscribe(filter, func(event *TableEvent) {
		eventMutex.Lock()
		eventCount++
		eventMutex.Unlock()
	})

	if err != nil {
		t.Fatalf("Failed to create subscription: %v", err)
	}

	// Performance test: publish many events
	numEvents := 1000
	start := time.Now()

	for i := 0; i < numEvents; i++ {
		event := &TableEvent{
			Type:      EventInsert,
			TableName: "players",
			Entity: TestPlayer{
				ID:   uint32(i),
				Name: "Player",
			},
		}
		bus.PublishEvent(event)
	}

	publishDuration := time.Since(start)

	// Wait for all events to be processed
	time.Sleep(100 * time.Millisecond)

	eventMutex.Lock()
	finalCount := eventCount
	eventMutex.Unlock()

	if finalCount != int64(numEvents) {
		t.Errorf("Expected %d events, got %d", numEvents, finalCount)
	}

	avgPublishTime := publishDuration.Nanoseconds() / int64(numEvents)
	t.Logf("Published %d events in %v (avg: %d ns per event)",
		numEvents, publishDuration, avgPublishTime)

	// Performance should be under 10 microseconds per event
	if avgPublishTime > 10000 {
		t.Errorf("Average publish time too slow: %d ns (expected < 10000 ns)", avgPublishTime)
	}
}

func TestCombinedFilter(t *testing.T) {
	bus := NewEventBus()
	defer bus.Shutdown()

	// Combined filter: players table, insert events, active players only
	var filteredEvents []*TableEvent
	var filterMutex sync.Mutex

	combinedFilter := CombinedFilter(
		[]string{"players"},
		[]EventType{EventInsert},
		func(event *TableEvent) bool {
			if player, ok := event.Entity.(TestPlayer); ok {
				return player.Active
			}
			return false
		},
	)

	_, err := bus.Subscribe(combinedFilter, func(event *TableEvent) {
		filterMutex.Lock()
		filteredEvents = append(filteredEvents, event)
		filterMutex.Unlock()
	})

	if err != nil {
		t.Fatalf("Failed to create combined subscription: %v", err)
	}

	// Test various events
	events := []*TableEvent{
		// Should match: players table, insert, active
		{Type: EventInsert, TableName: "players", Entity: TestPlayer{ID: 1, Name: "Alice", Active: true}},
		// Should not match: wrong table
		{Type: EventInsert, TableName: "games", Entity: TestGame{ID: 1, Name: "Game"}},
		// Should not match: wrong event type
		{Type: EventUpdate, TableName: "players", Entity: TestPlayer{ID: 2, Name: "Bob", Active: true}},
		// Should not match: inactive player
		{Type: EventInsert, TableName: "players", Entity: TestPlayer{ID: 3, Name: "Charlie", Active: false}},
		// Should match: players table, insert, active
		{Type: EventInsert, TableName: "players", Entity: TestPlayer{ID: 4, Name: "Diana", Active: true}},
	}

	for _, event := range events {
		bus.PublishEvent(event)
	}

	// Wait for processing
	time.Sleep(20 * time.Millisecond)

	filterMutex.Lock()
	matchedCount := len(filteredEvents)
	filterMutex.Unlock()

	if matchedCount != 2 {
		t.Errorf("Expected 2 matching events, got %d", matchedCount)
	}
}

// Benchmark tests

func BenchmarkEventPublish(b *testing.B) {
	bus := NewEventBus()
	defer bus.Shutdown()

	// Set up a subscription to ensure events are processed
	filter := TableFilter("players")
	bus.Subscribe(filter, func(event *TableEvent) {
		// Do nothing, just process
	})

	event := &TableEvent{
		Type:      EventInsert,
		TableName: "players",
		Entity:    TestPlayer{ID: 1, Name: "Player"},
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		bus.PublishEvent(event)
	}
}

func BenchmarkMultipleSubscriptions(b *testing.B) {
	bus := NewEventBus()
	defer bus.Shutdown()

	// Create multiple subscriptions
	numSubscriptions := 10
	filter := TableFilter("players")

	for i := 0; i < numSubscriptions; i++ {
		bus.Subscribe(filter, func(event *TableEvent) {
			// Process event
		})
	}

	event := &TableEvent{
		Type:      EventInsert,
		TableName: "players",
		Entity:    TestPlayer{ID: 1, Name: "Player"},
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		bus.PublishEvent(event)
	}
}

func BenchmarkBatchProcessing(b *testing.B) {
	bus := NewEventBus()
	defer bus.Shutdown()

	// Set up batch subscription
	filter := TableFilter("players")
	batchSize := 100
	timeout := 10 * time.Millisecond

	bus.SubscribeBatch(filter, func(events []*TableEvent) {
		// Process batch
	}, batchSize, timeout)

	event := &TableEvent{
		Type:      EventInsert,
		TableName: "players",
		Entity:    TestPlayer{ID: 1, Name: "Player"},
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		bus.PublishEvent(event)
	}
}
