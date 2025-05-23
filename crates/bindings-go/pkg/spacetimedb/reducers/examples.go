// Package reducers - Examples of using universal reducer patterns
package reducers

import (
	"fmt"
	"time"
)

// Example 1: Basic Game Setup using Universal Lifecycle Patterns
// This shows how a simple game would set up init/connect/disconnect reducers

func ExampleBasicGameSetup() {
	// Create a lifecycle builder with default settings
	builder := NewLifecycleReducerBuilder().
		WithPerformanceMonitoring(true).
		WithPlayerSessionManagement(true).
		WithAutoScheduleTimers(true).
		WithGameTickInterval(100 * time.Millisecond)

	// Add custom init logic for your game
	builder.OnInit(func(ctx *ReducerContext) error {
		LogInfo("Setting up game-specific initialization...")

		// Example: Initialize game configuration
		configManager := NewConfigurationManager(ctx)
		configManager.Set("max_players", 100)
		configManager.Set("world_size", 1000.0)
		configManager.Set("game_mode", "battle_royale")

		// Example: Set up game-specific database tables
		// This would use your game's specific table setup
		LogInfo("Game configuration initialized")
		return nil
	})

	// Add custom connect logic for your game
	builder.OnConnect(func(ctx *ReducerContext, identity *Identity) error {
		LogInfo(fmt.Sprintf("Player %s joining game", identity.Name))

		// Example: Create player record, assign spawn location, etc.
		// This would use your game's specific player setup

		return nil
	})

	// Add custom disconnect logic for your game
	builder.OnDisconnect(func(ctx *ReducerContext, identity *Identity) error {
		LogInfo(fmt.Sprintf("Player %s leaving game", identity.Name))

		// Example: Save player state, cleanup resources, etc.
		// This would use your game's specific cleanup

		return nil
	})

	// Build the universal reducers
	initReducer := builder.BuildInitReducer()
	connectReducer := builder.BuildConnectReducer()
	disconnectReducer := builder.BuildDisconnectReducer()

	// Register them with your reducer registry
	_ = initReducer
	_ = connectReducer
	_ = disconnectReducer

	LogInfo("Basic game setup complete")
}

// Example 2: Using Universal Utilities in Custom Reducers
// This shows how to use universal utilities in your game-specific reducers

func ExampleCustomReducerWithUtilities() *GenericReducer {
	return NewGenericReducer("PlayerAction", "Example player action reducer", func(ctx *ReducerContext, args []byte) ReducerResult {
		timer := NewPerformanceTimer("PlayerAction")
		defer timer.Stop()

		// Example: Parse and validate arguments using universal utilities
		type PlayerActionArgs struct {
			Action   string  `json:"action"`
			TargetX  float64 `json:"target_x"`
			TargetY  float64 `json:"target_y"`
			Strength float64 `json:"strength"`
		}

		var actionArgs PlayerActionArgs
		if err := UnmarshalArgs(args, &actionArgs); err != nil {
			return NewErrorResult(fmt.Errorf("Invalid arguments: %v", err))
		}

		// Example: Validate arguments using universal validators
		validator := NewArgumentValidator()

		// Validate action is one of allowed values
		allowedActions := []string{"move", "attack", "defend", "cast_spell"}
		if err := validator.ValidateEnum(actionArgs.Action, allowedActions); err != nil {
			return NewErrorResult(fmt.Errorf("Invalid action: %v", err))
		}

		// Validate coordinates are within world bounds
		if err := validator.ValidateRange(actionArgs.TargetX, -1000, 1000); err != nil {
			return NewErrorResult(fmt.Errorf("Invalid X coordinate: %v", err))
		}
		if err := validator.ValidateRange(actionArgs.TargetY, -1000, 1000); err != nil {
			return NewErrorResult(fmt.Errorf("Invalid Y coordinate: %v", err))
		}

		// Validate strength is reasonable
		if err := validator.ValidateRange(actionArgs.Strength, 0, 100); err != nil {
			return NewErrorResult(fmt.Errorf("Invalid strength: %v", err))
		}

		// Example: Use universal math utilities for game calculations
		clampedStrength := Clamp(float32(actionArgs.Strength), 0, 100)

		// Example: Use universal configuration manager
		configManager := NewConfigurationManager(ctx)
		maxActionRange := configManager.GetFloat("max_action_range", 50.0)

		// Calculate distance and validate range
		distance := actionArgs.TargetX*actionArgs.TargetX + actionArgs.TargetY*actionArgs.TargetY
		if distance > maxActionRange*maxActionRange {
			return NewErrorResult(fmt.Errorf("Target too far away"))
		}

		// Example: Use universal database operations with retry logic
		dbHelper := NewDatabaseOperationHelper(ctx)

		err := dbHelper.Retry(func() error {
			// Your game-specific database operations would go here
			LogInfo(fmt.Sprintf("Executing %s action at (%.1f, %.1f) with strength %.1f",
				actionArgs.Action, actionArgs.TargetX, actionArgs.TargetY, clampedStrength))
			return nil
		}, 3, time.Millisecond*100)

		if err != nil {
			return NewErrorResult(fmt.Errorf("Failed to execute action: %v", err))
		}

		return NewSuccessResult()
	})
}

// Example 3: Advanced Timer Management
// This shows how to use universal timer patterns for complex game systems

func ExampleAdvancedTimerManagement(ctx *ReducerContext) {
	// Create a timer scheduler with default 100ms interval
	scheduler := NewTimerScheduler(ctx, time.Millisecond*100)

	// Schedule a one-time event (e.g., boss spawn)
	bossSpawnArgs, _ := MarshalArgs(map[string]interface{}{
		"boss_type": "dragon",
		"location":  "center",
	})
	scheduler.ScheduleOnce("SpawnBoss", bossSpawnArgs, 5*time.Minute)

	// Schedule repeating events (e.g., resource spawning)
	resourceArgs, _ := MarshalArgs(map[string]interface{}{
		"resource_type": "gold",
		"amount":        100,
	})
	scheduler.ScheduleRepeating("SpawnResources", resourceArgs, 30*time.Second)

	// Schedule event at specific time (e.g., daily reset)
	tomorrow := time.Now().Add(24 * time.Hour)
	resetArgs, _ := MarshalArgs(map[string]interface{}{
		"reset_type": "daily",
	})
	scheduler.ScheduleAt("DailyReset", resetArgs, tomorrow)

	LogInfo("Advanced timer management configured")
}

// Example 4: Configuration Management Patterns
// This shows how to use universal configuration patterns for game settings

func ExampleConfigurationManagement(ctx *ReducerContext) {
	configManager := NewConfigurationManager(ctx)

	// Load default game configuration
	defaultConfig := map[string]interface{}{
		"world_size":       1000,
		"max_players":      64,
		"pvp_enabled":      true,
		"respawn_time":     "30s",
		"difficulty_level": "normal",
		"weather_enabled":  true,
		"day_night_cycle":  "24m",
	}

	// Convert to JSON and load
	configData, _ := MarshalArgs(defaultConfig)
	configManager.LoadFromJSON(configData)

	// Access configuration values with type safety and defaults
	worldSize := configManager.GetInt("world_size", 500)
	maxPlayers := configManager.GetInt("max_players", 32)
	pvpEnabled := configManager.GetBool("pvp_enabled", false)
	respawnTime := configManager.GetDuration("respawn_time", 10*time.Second)
	difficulty := configManager.GetString("difficulty_level", "easy")

	LogInfo(fmt.Sprintf("Game configured: world=%d, players=%d, pvp=%t, respawn=%v, difficulty=%s",
		worldSize, maxPlayers, pvpEnabled, respawnTime, difficulty))

	// Configuration can be modified at runtime
	configManager.Set("pvp_enabled", false)
	configManager.Set("max_players", 32)

	// Save configuration (could be persisted to database)
	savedConfig, _ := configManager.SaveToJSON()
	LogInfo(fmt.Sprintf("Configuration saved: %d bytes", len(savedConfig)))
}

// Example 5: Database Operation Patterns
// This shows how to use universal database patterns for reliable operations

func ExampleDatabaseOperations(ctx *ReducerContext) {
	dbHelper := NewDatabaseOperationHelper(ctx)

	// Example: Batch operations for efficiency
	operations := []func() error{
		func() error {
			LogInfo("Creating player record...")
			// Your database insert operation
			return nil
		},
		func() error {
			LogInfo("Updating player stats...")
			// Your database update operation
			return nil
		},
		func() error {
			LogInfo("Recording achievement...")
			// Your database insert operation
			return nil
		},
	}

	if err := dbHelper.BatchOperation(operations); err != nil {
		LogError(fmt.Sprintf("Batch operation failed: %v", err))
	}

	// Example: Transaction-like operations
	err := dbHelper.WithTransaction(func() error {
		LogInfo("Starting complex multi-table operation...")

		// Multiple related database operations that should succeed or fail together
		// In a real implementation, these would be actual database calls

		LogInfo("All operations completed successfully")
		return nil
	})

	if err != nil {
		LogError(fmt.Sprintf("Transaction failed: %v", err))
	}

	// Example: Retry logic for unreliable operations
	err = dbHelper.Retry(func() error {
		// Simulate an operation that might fail intermittently
		LogInfo("Attempting network-dependent operation...")
		// Your potentially failing operation
		return nil
	}, 3, time.Millisecond*500)

	if err != nil {
		LogError(fmt.Sprintf("Operation failed after retries: %v", err))
	}
}

// Example 6: Player Session Management
// This shows how to use universal player session patterns

func ExamplePlayerSessionManagement(ctx *ReducerContext, identity *Identity) {
	sessionManager := NewPlayerSessionManager(ctx).
		WithSessionRestoration(true).
		WithSessionTimeout(30 * time.Minute)

	// Try to restore an existing session
	restored, err := sessionManager.RestoreSession(identity)
	if err != nil {
		LogError(fmt.Sprintf("Session restoration failed: %v", err))
		return
	}

	if restored {
		LogInfo(fmt.Sprintf("Session restored for player: %s", identity.Name))
	} else {
		LogInfo(fmt.Sprintf("Starting new session for player: %s", identity.Name))

		// Start a new session
		if err := sessionManager.StartSession(identity); err != nil {
			LogError(fmt.Sprintf("Failed to start session: %v", err))
			return
		}
	}

	// Later, when player disconnects
	if err := sessionManager.EndSession(identity, "player_quit"); err != nil {
		LogError(fmt.Sprintf("Failed to end session: %v", err))
	}
}

// Example 7: Argument Validation Patterns
// This shows comprehensive argument validation patterns

func ExampleArgumentValidation() {
	validator := NewArgumentValidator()

	// Example player data to validate
	type PlayerData struct {
		Name   string   `json:"name"`
		Level  int      `json:"level"`
		Health float64  `json:"health"`
		Class  string   `json:"class"`
		Items  []string `json:"items"`
	}

	playerData := PlayerData{
		Name:   "TestPlayer",
		Level:  15,
		Health: 85.5,
		Class:  "warrior",
		Items:  []string{"sword", "shield"},
	}

	// Validate name length
	if err := validator.ValidateStringLength(playerData.Name, 3, 20); err != nil {
		LogError(fmt.Sprintf("Invalid player name: %v", err))
		return
	}

	// Validate level range
	if err := validator.ValidateRange(float64(playerData.Level), 1, 100); err != nil {
		LogError(fmt.Sprintf("Invalid player level: %v", err))
		return
	}

	// Validate health percentage
	if err := validator.ValidateRange(playerData.Health, 0, 100); err != nil {
		LogError(fmt.Sprintf("Invalid player health: %v", err))
		return
	}

	// Validate class is allowed
	allowedClasses := []string{"warrior", "mage", "archer", "rogue"}
	if err := validator.ValidateEnum(playerData.Class, allowedClasses); err != nil {
		LogError(fmt.Sprintf("Invalid player class: %v", err))
		return
	}

	// Validate required fields using reflection
	requiredFields := []string{"Name", "Level", "Class"}
	if err := ValidateRequired(playerData, requiredFields); err != nil {
		LogError(fmt.Sprintf("Missing required fields: %v", err))
		return
	}

	LogInfo("Player data validation passed")
}

// Example 8: Math Utilities for Game Calculations
// This shows how to use universal math utilities for common game calculations

func ExampleMathUtilities() {
	// Example: Damage calculation with clamping
	baseDamage := float32(45.7)
	damageMultiplier := float32(1.8)
	calculatedDamage := baseDamage * damageMultiplier

	// Clamp damage to reasonable bounds
	finalDamage := Clamp(calculatedDamage, 1.0, 100.0)
	LogInfo(fmt.Sprintf("Damage: %.1f -> %.1f (clamped)", calculatedDamage, finalDamage))

	// Example: Player level progression with clamping
	currentXP := 15750
	requiredXP := 10000
	maxLevel := 50
	newLevel := ClampInt(currentXP/requiredXP, 1, maxLevel)
	LogInfo(fmt.Sprintf("Player level: %d (max: %d)", newLevel, maxLevel))

	// Example: Smooth movement interpolation
	startPos := float32(10.0)
	endPos := float32(50.0)
	progress := float32(0.3) // 30% of the way
	currentPos := Lerp(startPos, endPos, progress)
	LogInfo(fmt.Sprintf("Position interpolation: %.1f -> %.1f (%.1f)", startPos, endPos, currentPos))

	// Example: Converting between different ranges (e.g., health percentage to bar width)
	healthPercentage := float32(75.0)                      // 75% health
	barWidth := MapRange(healthPercentage, 0, 100, 0, 200) // Map to 200px bar
	LogInfo(fmt.Sprintf("Health bar: %.1f%% -> %.1fpx", healthPercentage, barWidth))
}

// RunAllExamples demonstrates all the universal reducer patterns
func RunAllExamples() {
	LogInfo("=== Universal Reducer Patterns Examples ===")

	LogInfo("1. Basic Game Setup")
	ExampleBasicGameSetup()

	LogInfo("2. Custom Reducer with Utilities")
	customReducer := ExampleCustomReducerWithUtilities()
	LogInfo(fmt.Sprintf("Created reducer: %s", customReducer.Name()))

	// The following examples would need a real ReducerContext
	// LogInfo("3. Advanced Timer Management")
	// ExampleAdvancedTimerManagement(ctx)

	// LogInfo("4. Configuration Management")
	// ExampleConfigurationManagement(ctx)

	// LogInfo("5. Database Operations")
	// ExampleDatabaseOperations(ctx)

	// LogInfo("6. Player Session Management")
	// ExamplePlayerSessionManagement(ctx, identity)

	LogInfo("7. Argument Validation")
	ExampleArgumentValidation()

	LogInfo("8. Math Utilities")
	ExampleMathUtilities()

	LogInfo("=== All Examples Completed ===")
}
