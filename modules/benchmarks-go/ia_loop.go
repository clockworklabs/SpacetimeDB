package main

import (
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/log"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/reducer"
)

// AgentAction is a simple enum for enemy AI agent actions.
//
//stdb:enum variants=Inactive,Idle,Evading,Investigating,Retreating,Fighting
type AgentAction uint8

const (
	AgentActionInactive      AgentAction = iota
	AgentActionIdle
	AgentActionEvading
	AgentActionInvestigating
	AgentActionRetreating
	AgentActionFighting
)

// SmallHexTile is a helper struct for herd cache locations.
type SmallHexTile struct {
	X         int32
	Z         int32
	Dimension uint32
}

//stdb:table name=velocity access=public
type Velocity struct {
	EntityId uint32 `stdb:"primarykey"`
	X        float32
	Y        float32
	Z        float32
}

//stdb:table name=position access=public
type Position struct {
	EntityId uint32 `stdb:"primarykey"`
	X        float32
	Y        float32
	Z        float32
	Vx       float32
	Vy       float32
	Vz       float32
}

//stdb:table name=game_enemy_ai_agent_state access=public
type GameEnemyAiAgentState struct {
	EntityId            uint64 `stdb:"primarykey"`
	LastMoveTimestamps  []uint64
	NextActionTimestamp uint64
	Action              AgentAction
}

//stdb:table name=game_targetable_state access=public
type GameTargetableState struct {
	EntityId uint64 `stdb:"primarykey"`
	Quad     int64
}

//stdb:table name=game_live_targetable_state access=public
type GameLiveTargetableState struct {
	EntityId uint64 `stdb:"unique"`
	Quad     int64  `stdb:"index=btree"`
}

//stdb:table name=game_mobile_entity_state access=public
type GameMobileEntityState struct {
	EntityId  uint64 `stdb:"primarykey"`
	LocationX int32  `stdb:"index=btree"`
	LocationY int32
	Timestamp uint64
}

//stdb:table name=game_enemy_state access=public
type GameEnemyState struct {
	EntityId uint64 `stdb:"primarykey"`
	HerdId   int32
}

//stdb:table name=game_herd_cache access=public
type GameHerdCache struct {
	Id                int32 `stdb:"primarykey"`
	DimensionId       uint32
	CurrentPopulation int32
	Location          SmallHexTile
	MaxPopulation     int32
	SpawnEagerness    float32
	RoamingDistance    int32
}

var iaLogger log.Logger

func init() {
	iaLogger = log.NewLogger("ia_loop")
}

//stdb:reducer name=insert_bulk_position
func insertBulkPosition(ctx reducer.ReducerContext, count uint32) {
	for i := uint32(0); i < count; i++ {
		PositionTable.Insert(Position{
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

//stdb:reducer name=insert_bulk_velocity
func insertBulkVelocity(ctx reducer.ReducerContext, count uint32) {
	for i := uint32(0); i < count; i++ {
		VelocityTable.Insert(Velocity{
			EntityId: i,
			X:        0.1,
			Y:        0.2,
			Z:        0.3,
		})
	}
	iaLogger.Info(fmt.Sprintf("INSERT VELOCITY: %d", count))
}

//stdb:reducer name=insert_world
func insertWorld(ctx reducer.ReducerContext, players uint64) {
	for i := uint64(0); i < players; i++ {
		GameEnemyAiAgentStateTable.Insert(GameEnemyAiAgentState{
			EntityId:            i,
			LastMoveTimestamps:  []uint64{0, 0, 0, 0, 0},
			NextActionTimestamp: 100 + i,
			Action:              AgentActionIdle,
		})

		GameLiveTargetableStateTable.Insert(GameLiveTargetableState{
			EntityId: i,
			Quad:     int64(i) % 4,
		})

		GameTargetableStateTable.Insert(GameTargetableState{
			EntityId: i,
			Quad:     int64(i) % 4,
		})

		GameMobileEntityStateTable.Insert(GameMobileEntityState{
			EntityId:  i,
			LocationX: int32(i),
			LocationY: int32(i * 2),
			Timestamp: 1000,
		})

		GameEnemyStateTable.Insert(GameEnemyState{
			EntityId: i,
			HerdId:   int32(i) % 10,
		})
	}

	// Insert 10 herds
	for h := int32(0); h < 10; h++ {
		GameHerdCacheTable.Insert(GameHerdCache{
			Id:                h,
			DimensionId:       0,
			CurrentPopulation: int32(players / 10),
			Location: SmallHexTile{
				X:         h * 10,
				Z:         h * 20,
				Dimension: 0,
			},
			MaxPopulation:  100,
			SpawnEagerness: 0.5,
			RoamingDistance: 10,
		})
	}

	iaLogger.Info(fmt.Sprintf("INSERT WORLD PLAYERS: %d", players))
}

//stdb:reducer name=update_position_all
func updatePositionAll(ctx reducer.ReducerContext, expected uint32) {
	count := uint32(0)
	iter, err := PositionTable.Scan()
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
		PositionTable.UpdateByEntityId(pos)
		count++
	}
	iaLogger.Info(fmt.Sprintf("UPDATE POSITION ALL: %d, processed: %d", expected, count))
}

//stdb:reducer name=update_position_with_velocity
func updatePositionWithVelocity(ctx reducer.ReducerContext, expected uint32) {
	count := uint32(0)
	iter, err := PositionTable.Scan()
	if err != nil {
		return
	}
	defer iter.Close()

	for {
		pos, ok := iter.Next()
		if !ok {
			break
		}
		vel, found, err := VelocityTable.FindByEntityId(pos.EntityId)
		if err != nil || !found {
			continue
		}
		pos.X += vel.X
		pos.Y += vel.Y
		pos.Z += vel.Z
		PositionTable.UpdateByEntityId(pos)
		count++
	}
	iaLogger.Info(fmt.Sprintf("UPDATE POSITION BY VELOCITY: %d, processed: %d", expected, count))
}

//stdb:reducer name=game_loop_enemy_ia
func gameLoopEnemyIA(ctx reducer.ReducerContext, players uint64) {
	count := uint64(0)
	iter, err := GameEnemyAiAgentStateTable.Scan()
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

		targetable, found, err := GameTargetableStateTable.FindByEntityId(agent.EntityId)
		if err != nil || !found {
			continue
		}
		_ = targetable

		liveTargetable, found, err := GameLiveTargetableStateTable.FindByEntityId(agent.EntityId)
		if err != nil || !found {
			continue
		}
		_ = liveTargetable

		mobile, found, err := GameMobileEntityStateTable.FindByEntityId(agent.EntityId)
		if err != nil || !found {
			continue
		}
		_ = mobile

		enemy, found, err := GameEnemyStateTable.FindByEntityId(agent.EntityId)
		if err != nil || !found {
			continue
		}

		herd, found, err := GameHerdCacheTable.FindById(enemy.HerdId)
		if err != nil || !found {
			continue
		}
		_ = herd

		count++
	}
	iaLogger.Info(fmt.Sprintf("ENEMY IA LOOP PLAYERS: %d, processed: %d", players, count))
}

//stdb:reducer name=init_game_ia_loop
func initGameIALoop(ctx reducer.ReducerContext, initialLoad uint32) {
	bigTable := initialLoad * 50
	smallTable := uint64(initialLoad)

	insertBulkPosition(ctx, bigTable)
	insertBulkVelocity(ctx, bigTable)
	updatePositionAll(ctx, bigTable)
	updatePositionWithVelocity(ctx, bigTable)
	insertWorld(ctx, smallTable)
}

//stdb:reducer name=run_game_ia_loop
func runGameIALoop(ctx reducer.ReducerContext, initialLoad uint32) {
	gameLoopEnemyIA(ctx, uint64(initialLoad))
}
