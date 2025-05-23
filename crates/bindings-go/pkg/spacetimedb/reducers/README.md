# SpacetimeDB Go Reducer Framework

A comprehensive framework for building robust, scalable game reducers with SpacetimeDB using Go. This package provides universal patterns and utilities that can be reused across different games while keeping game-specific logic separate.

## 🎯 Philosophy: Universal vs. Game-Specific

This framework follows a clear separation of concerns:

- **Universal (`@pkg`)**: Common patterns, utilities, and infrastructure that can benefit any SpacetimeDB game
- **Game-Specific (`@server-go`)**: Your specific game logic, schemas, and business rules

## 📦 Package Structure

```
pkg/spacetimedb/reducers/
├── framework.go     # Core reducer framework and registry
├── lifecycle.go     # Universal lifecycle reducer patterns (init/connect/disconnect)
├── utilities.go     # Common utilities (validation, config, database operations)
├── examples.go      # Comprehensive usage examples
└── README.md       # This file
```

## 🚀 Quick Start

### 1. Basic Game Setup

```go
import "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb/reducers"

// Create lifecycle reducers with universal patterns
builder := reducers.NewLifecycleReducerBuilder().
    WithPerformanceMonitoring(true).
    WithPlayerSessionManagement(true).
    WithAutoScheduleTimers(true).
    WithGameTickInterval(100 * time.Millisecond)

// Add your game-specific init logic
builder.OnInit(func(ctx *reducers.ReducerContext) error {
    // Initialize your game configuration
    // Set up your database tables
    // Configure your game world
    return nil
})

// Build universal reducers with your custom logic
initReducer := builder.BuildInitReducer()
connectReducer := builder.BuildConnectReducer()
disconnectReducer := builder.BuildDisconnectReducer()
```

### 2. Custom Game Reducers

```go
// Use universal utilities in your game-specific reducers
func PlayerMoveReducer(ctx *ReducerContext, args []byte) ReducerResult {
    timer := reducers.NewPerformanceTimer("PlayerMove")
    defer timer.Stop()
    
    // Parse arguments with validation
    type MoveArgs struct {
        X float64 `json:"x"`
        Y float64 `json:"y"`
    }
    
    var moveArgs MoveArgs
    if err := reducers.UnmarshalArgs(args, &moveArgs); err != nil {
        return reducers.NewErrorResult(err)
    }
    
    // Validate with universal validators
    validator := reducers.NewArgumentValidator()
    if err := validator.ValidateRange(moveArgs.X, -1000, 1000); err != nil {
        return reducers.NewErrorResult(err)
    }
    
    // Your game-specific movement logic here
    
    return reducers.NewSuccessResult()
}
```

## 🏗️ Core Components

### Lifecycle Reducer Builder

The `LifecycleReducerBuilder` provides a fluent API for creating standardized init, connect, and disconnect reducers:

```go
builder := reducers.NewLifecycleReducerBuilder()

// Configuration
builder.WithPerformanceMonitoring(true)        // Enable performance timers
builder.WithPlayerSessionManagement(true)      // Enable session restoration
builder.WithAutoScheduleTimers(true)           // Auto-schedule game timers
builder.WithGameTickInterval(100 * time.Millisecond)
builder.WithCleanupInterval(time.Minute)

// Custom handlers
builder.OnInit(func(ctx *ReducerContext) error { /* your init logic */ })
builder.OnConnect(func(ctx *ReducerContext, identity *Identity) error { /* your connect logic */ })
builder.OnDisconnect(func(ctx *ReducerContext, identity *Identity) error { /* your disconnect logic */ })

// Build reducers
initReducer := builder.BuildInitReducer()
connectReducer := builder.BuildConnectReducer()
disconnectReducer := builder.BuildDisconnectReducer()
```

### Universal Utilities

#### Argument Parsing & Validation

```go
// Parse JSON arguments
var args MyArgsStruct
if err := reducers.UnmarshalArgs(data, &args); err != nil {
    return reducers.NewErrorResult(err)
}

// Validate arguments
validator := reducers.NewArgumentValidator()
validator.ValidateStringLength(name, 3, 20)
validator.ValidateRange(health, 0, 100)
validator.ValidateEnum(class, []string{"warrior", "mage", "archer"})
validator.ValidateRequired(args, []string{"Name", "Level"})
```

#### Configuration Management

```go
config := reducers.NewConfigurationManager(ctx)

// Load from JSON
config.LoadFromJSON(configData)

// Type-safe access with defaults
worldSize := config.GetInt("world_size", 1000)
pvpEnabled := config.GetBool("pvp_enabled", false)
respawnTime := config.GetDuration("respawn_time", 30*time.Second)

// Runtime modification
config.Set("max_players", 64)
```

#### Timer Scheduling

```go
scheduler := reducers.NewTimerScheduler(ctx, time.Millisecond*100)

// Schedule one-time events
scheduler.ScheduleOnce("SpawnBoss", args, 5*time.Minute)

// Schedule repeating events
scheduler.ScheduleRepeating("GameTick", args, 100*time.Millisecond)

// Schedule at specific time
scheduler.ScheduleAt("DailyReset", args, tomorrow)
```

#### Database Operations

```go
dbHelper := reducers.NewDatabaseOperationHelper(ctx)

// Batch operations
operations := []func() error{
    func() error { /* operation 1 */ return nil },
    func() error { /* operation 2 */ return nil },
}
dbHelper.BatchOperation(operations)

// Transaction-like operations
dbHelper.WithTransaction(func() error {
    // Multiple related operations
    return nil
})

// Retry logic
dbHelper.Retry(func() error {
    // Potentially failing operation
    return nil
}, 3, time.Millisecond*500)
```

#### Player Session Management

```go
sessionManager := reducers.NewPlayerSessionManager(ctx).
    WithSessionRestoration(true).
    WithSessionTimeout(30 * time.Minute)

// Try to restore existing session
restored, err := sessionManager.RestoreSession(identity)
if !restored {
    sessionManager.StartSession(identity)
}

// End session
sessionManager.EndSession(identity, "player_quit")
```

#### Math Utilities

```go
// Clamp values to bounds
damage := reducers.Clamp(calculatedDamage, 1.0, 100.0)
level := reducers.ClampInt(xp/1000, 1, 50)

// Interpolation
position := reducers.Lerp(startPos, endPos, 0.3)

// Range mapping
barWidth := reducers.MapRange(healthPercent, 0, 100, 0, 200)
```

## 🎮 Integration with Game-Specific Code

### Recommended Project Structure

```
your-game/
├── server-go/                          # Game-specific code
│   ├── main.go                         # Main entry point
│   ├── reducers/                       # Game-specific reducers
│   │   ├── game_reducers.go           # Your game logic
│   │   └── game_lifecycle.go          # Game-specific lifecycle
│   ├── tables/                         # Game-specific database schemas
│   ├── logic/                          # Game business logic
│   ├── constants/                      # Game constants
│   └── types/                          # Game-specific types
└── pkg/                                # Universal utilities (from SpacetimeDB)
    └── spacetimedb/reducers/           # Universal reducer framework
```

### Game-Specific Implementation Example

```go
// In your server-go/reducers/game_lifecycle.go
package reducers

import (
    "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb/reducers"
    "../tables"
    "../constants"
)

func SetupGameLifecycle() {
    builder := reducers.NewLifecycleReducerBuilder()
    
    // Add Blackholio-specific init logic
    builder.OnInit(func(ctx *reducers.ReducerContext) error {
        // Initialize Blackholio configuration
        config := tables.NewConfig(0, constants.DEFAULT_WORLD_SIZE)
        return ctx.Database.InsertConfig(config)
    })
    
    // Add Blackholio-specific connect logic
    builder.OnConnect(func(ctx *reducers.ReducerContext, identity *reducers.Identity) error {
        // Create Blackholio player
        player := tables.NewPlayer(identity, 0, "")
        return ctx.Database.InsertPlayer(player)
    })
    
    // Register the reducers
    Register(builder.BuildInitReducer())
    Register(builder.BuildConnectReducer()) 
    Register(builder.BuildDisconnectReducer())
}
```

## 🏆 Best Practices

### 1. Separation of Concerns

- **Universal patterns** go in `@pkg` (this package)
- **Game-specific logic** stays in your `@server-go`
- Use the universal utilities as building blocks for your game logic

### 2. Error Handling

```go
// Always use the universal error patterns
return reducers.NewErrorResult(fmt.Errorf("operation failed: %w", err))
return reducers.NewSuccessResult()
```

### 3. Performance Monitoring

```go
// Use performance timers for important operations
timer := reducers.NewPerformanceTimer("ExpensiveOperation")
defer timer.Stop()
```

### 4. Argument Validation

```go
// Always validate arguments before processing
validator := reducers.NewArgumentValidator()
if err := validator.ValidateRange(damage, 0, 1000); err != nil {
    return reducers.NewErrorResult(err)
}
```

### 5. Configuration Management

```go
// Use the configuration manager for runtime settings
config := reducers.NewConfigurationManager(ctx)
maxPlayers := config.GetInt("max_players", 32)
```

## 📚 Examples

See `examples.go` for comprehensive examples of:

- Basic game setup patterns
- Custom reducer implementation
- Advanced timer management
- Configuration management
- Database operation patterns
- Player session management
- Argument validation
- Math utilities

Run the examples:

```go
import "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb/reducers"

reducers.RunAllExamples()
```

## 🔧 Advanced Usage

### Custom Reducer Registration

```go
// Register your game-specific reducers
func RegisterGameReducers() {
    Register(NewGenericReducer("PlayerMove", "Move player", PlayerMoveReducer))
    Register(NewGenericReducer("PlayerAttack", "Attack action", PlayerAttackReducer))
    Register(NewGenericReducer("SpawnItem", "Spawn item", SpawnItemReducer))
}
```

### Custom Validation

```go
// Extend the argument validator for game-specific validation
func ValidatePlayerClass(class string) error {
    allowedClasses := []string{"warrior", "mage", "archer", "rogue"}
    validator := reducers.NewArgumentValidator()
    return validator.ValidateEnum(class, allowedClasses)
}
```

### Custom Configuration

```go
// Extend configuration manager for game-specific settings
func LoadGameConfiguration(ctx *ReducerContext) error {
    config := reducers.NewConfigurationManager(ctx)
    
    gameConfig := map[string]interface{}{
        "world_size": 1000,
        "max_players": 64,
        "pvp_enabled": true,
    }
    
    configData, _ := json.Marshal(gameConfig)
    return config.LoadFromJSON(configData)
}
```

## 🚀 Performance Considerations

- Use performance timers to identify bottlenecks
- Batch database operations when possible
- Use retry logic for unreliable operations
- Clamp values to prevent extreme calculations
- Use the configuration manager for runtime adjustments

## 🤝 Contributing

When adding new universal patterns:

1. Ensure they're truly universal (useful across multiple games)
2. Keep game-specific logic out of the universal package
3. Add comprehensive examples
4. Include proper error handling
5. Add performance monitoring
6. Update this README

## 📖 Related Documentation

- [SpacetimeDB Documentation](https://spacetimedb.com/docs)
- [Go Language Specification](https://golang.org/ref/spec)
- [Game Development Best Practices](https://spacetimedb.com/docs/game-development)

---

**Built with ❤️ for the SpacetimeDB community** 