package main

import (
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/log"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/runtime"
)

// AgentAction is a sum-type enum for enemy AI agent actions.
type AgentAction uint8

const (
	AgentActionInactive      AgentAction = 0
	AgentActionIdle          AgentAction = 1
	AgentActionEvading       AgentAction = 2
	AgentActionInvestigating AgentAction = 3
	AgentActionRetreating    AgentAction = 4
	AgentActionFighting      AgentAction = 5
)

// SmallHexTile is a helper struct for herd cache locations.
type SmallHexTile struct {
	X         int32
	Z         int32
	Dimension uint32
}

// Velocity table
type Velocity struct {
	EntityId uint32 `stdb:"primarykey"`
	X        float32
	Y        float32
	Z        float32
}

// Position table
type Position struct {
	EntityId uint32 `stdb:"primarykey"`
	X        float32
	Y        float32
	Z        float32
	Vx       float32
	Vy       float32
	Vz       float32
}

// GameEnemyAiAgentState table
type GameEnemyAiAgentState struct {
	EntityId            uint64 `stdb:"primarykey"`
	LastMoveTimestamps  []uint64
	NextActionTimestamp uint64
	Action              AgentAction
}

// GameTargetableState table
type GameTargetableState struct {
	EntityId uint64 `stdb:"primarykey"`
	Quad     int64
}

// GameLiveTargetableState table
type GameLiveTargetableState struct {
	EntityId uint64 `stdb:"unique"`
	Quad     int64  `stdb:"index=btree"`
}

// GameMobileEntityState table
type GameMobileEntityState struct {
	EntityId  uint64 `stdb:"primarykey"`
	LocationX int32  `stdb:"index=btree"`
	LocationY int32
	Timestamp uint64
}

// GameEnemyState table
type GameEnemyState struct {
	EntityId uint64 `stdb:"primarykey"`
	HerdId   int32
}

// GameHerdCache table
type GameHerdCache struct {
	Id                int32 `stdb:"primarykey"`
	DimensionId       uint32
	CurrentPopulation int32
	Location          SmallHexTile
	MaxPopulation     int32
	SpawnEagerness    float32
	RoamingDistance   int32
}

// Index name constants for FindBy/UpdateBy/DeleteBy operations.
const (
	velocityEntityIdIdx                = "velocity_entity_id_idx_btree"
	positionEntityIdIdx                = "position_entity_id_idx_btree"
	gameEnemyAiAgentStateEntityIdIdx   = "game_enemy_ai_agent_state_entity_id_idx_btree"
	gameTargetableStateEntityIdIdx     = "game_targetable_state_entity_id_idx_btree"
	gameLiveTargetableStateEntityIdIdx = "game_live_targetable_state_entity_id_idx_btree"
	gameMobileEntityStateEntityIdIdx   = "game_mobile_entity_state_entity_id_idx_btree"
	gameMobileEntityStateLocationXIdx  = "game_mobile_entity_state_location_x_idx_btree"
	gameEnemyStateEntityIdIdx          = "game_enemy_state_entity_id_idx_btree"
	gameHerdCacheIdIdx                 = "game_herd_cache_id_idx_btree"
)

var iaLogger log.Logger

func init() {
	iaLogger = log.NewLogger("ia_loop")

	// Register tables
	server.RegisterTable[Velocity]("velocity", server.TableAccessPublic)
	server.RegisterTable[Position]("position", server.TableAccessPublic)
	server.RegisterTable[GameEnemyAiAgentState]("game_enemy_ai_agent_state", server.TableAccessPublic)
	server.RegisterTable[GameTargetableState]("game_targetable_state", server.TableAccessPublic)
	server.RegisterTable[GameLiveTargetableState]("game_live_targetable_state", server.TableAccessPublic)
	server.RegisterTable[GameMobileEntityState]("game_mobile_entity_state", server.TableAccessPublic)
	server.RegisterTable[GameEnemyState]("game_enemy_state", server.TableAccessPublic)
	server.RegisterTable[GameHerdCache]("game_herd_cache", server.TableAccessPublic)

	// Register reducers
	server.RegisterReducer("insert_bulk_position", insertBulkPosition)
	server.RegisterReducer("insert_bulk_velocity", insertBulkVelocity)
	server.RegisterReducer("insert_world", insertWorld)
	server.RegisterReducer("update_position_all", updatePositionAll)
	server.RegisterReducer("update_position_with_velocity", updatePositionWithVelocity)
	server.RegisterReducer("game_loop_enemy_ia", gameLoopEnemyIA)
	server.RegisterReducer("init_game_ia_loop", initGameIALoop)
	server.RegisterReducer("run_game_ia_loop", runGameIALoop)
}

func insertBulkPosition(ctx server.ReducerContext, count uint32) {
	for i := uint32(0); i < count; i++ {
		runtime.Insert(Position{
			EntityId: i,
			X:        float32(i),
			Y:        float32(i + 10),
			Z:        0.0,
			Vx:       1.0,
			Vy:       2.0,
			Vz:       0.5,
		})
	}
	iaLogger.Info(fmt.Sprintf("INSERT POSITION: %d", count))
}

func insertBulkVelocity(ctx server.ReducerContext, count uint32) {
	for i := uint32(0); i < count; i++ {
		runtime.Insert(Velocity{
			EntityId: i,
			X:        0.1,
			Y:        0.2,
			Z:        0.3,
		})
	}
	iaLogger.Info(fmt.Sprintf("INSERT VELOCITY: %d", count))
}

func insertWorld(ctx server.ReducerContext, players uint64) {
	for i := uint64(0); i < players; i++ {
		runtime.Insert(GameEnemyAiAgentState{
			EntityId:            i,
			LastMoveTimestamps:  []uint64{0, 0, 0, 0, 0},
			NextActionTimestamp: 100 + i,
			Action:              AgentActionIdle,
		})

		runtime.Insert(GameLiveTargetableState{
			EntityId: i,
			Quad:     int64(i) % 4,
		})

		runtime.Insert(GameTargetableState{
			EntityId: i,
			Quad:     int64(i) % 4,
		})

		runtime.Insert(GameMobileEntityState{
			EntityId:  i,
			LocationX: int32(i),
			LocationY: int32(i * 2),
			Timestamp: 1000,
		})

		runtime.Insert(GameEnemyState{
			EntityId: i,
			HerdId:   int32(i) % 10,
		})
	}

	// Insert 10 herds
	for h := int32(0); h < 10; h++ {
		runtime.Insert(GameHerdCache{
			Id:                h,
			DimensionId:       0,
			CurrentPopulation: int32(players / 10),
			Location: SmallHexTile{
				X:         h * 10,
				Z:         h * 20,
				Dimension: 0,
			},
			MaxPopulation:   100,
			SpawnEagerness:  0.5,
			RoamingDistance: 10,
		})
	}

	iaLogger.Info(fmt.Sprintf("INSERT WORLD PLAYERS: %d", players))
}

func updatePositionAll(ctx server.ReducerContext, expected uint32) {
	count := uint32(0)
	iter, err := runtime.Scan[Position]()
	if err != nil {
		return
	}
	defer iter.Close()

	for {
		pos, ok := iter.Next()
		if !ok {
			break
		}
		pos.X += pos.Vx
		pos.Y += pos.Vy
		pos.Z += pos.Vz
		runtime.UpdateBy[Position](positionEntityIdIdx, pos)
		count++
	}
	iaLogger.Info(fmt.Sprintf("UPDATE POSITION ALL: %d, processed: %d", expected, count))
}

func updatePositionWithVelocity(ctx server.ReducerContext, expected uint32) {
	count := uint32(0)
	iter, err := runtime.Scan[Position]()
	if err != nil {
		return
	}
	defer iter.Close()

	for {
		pos, ok := iter.Next()
		if !ok {
			break
		}
		vel, found, err := runtime.FindBy[Velocity, uint32](velocityEntityIdIdx, pos.EntityId)
		if err != nil || !found {
			continue
		}
		pos.X += vel.X
		pos.Y += vel.Y
		pos.Z += vel.Z
		runtime.UpdateBy[Position](positionEntityIdIdx, pos)
		count++
	}
	iaLogger.Info(fmt.Sprintf("UPDATE POSITION BY VELOCITY: %d, processed: %d", expected, count))
}

func gameLoopEnemyIA(ctx server.ReducerContext, players uint64) {
	count := uint64(0)
	iter, err := runtime.Scan[GameEnemyAiAgentState]()
	if err != nil {
		return
	}
	defer iter.Close()

	for {
		agent, ok := iter.Next()
		if !ok {
			break
		}
		_ = agent

		targetable, found, err := runtime.FindBy[GameTargetableState, uint64](gameTargetableStateEntityIdIdx, agent.EntityId)
		if err != nil || !found {
			continue
		}
		_ = targetable

		liveTargetable, found, err := runtime.FindBy[GameLiveTargetableState, uint64](gameLiveTargetableStateEntityIdIdx, agent.EntityId)
		if err != nil || !found {
			continue
		}
		_ = liveTargetable

		mobile, found, err := runtime.FindBy[GameMobileEntityState, uint64](gameMobileEntityStateEntityIdIdx, agent.EntityId)
		if err != nil || !found {
			continue
		}
		_ = mobile

		enemy, found, err := runtime.FindBy[GameEnemyState, uint64](gameEnemyStateEntityIdIdx, agent.EntityId)
		if err != nil || !found {
			continue
		}

		herd, found, err := runtime.FindBy[GameHerdCache, int32](gameHerdCacheIdIdx, enemy.HerdId)
		if err != nil || !found {
			continue
		}
		_ = herd

		count++
	}
	iaLogger.Info(fmt.Sprintf("ENEMY IA LOOP PLAYERS: %d, processed: %d", players, count))
}

func initGameIALoop(ctx server.ReducerContext, initialLoad uint32) {
	bigTable := initialLoad * 50
	smallTable := uint64(initialLoad)

	insertBulkPosition(ctx, bigTable)
	insertBulkVelocity(ctx, bigTable)
	updatePositionAll(ctx, bigTable)
	updatePositionWithVelocity(ctx, bigTable)
	insertWorld(ctx, smallTable)
}

func runGameIALoop(ctx server.ReducerContext, initialLoad uint32) {
	gameLoopEnemyIA(ctx, uint64(initialLoad))
}
