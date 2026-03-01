package main

import (
	"fmt"
	"math"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/log"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/runtime"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// ---------- helper struct ----------

type Vector2 struct {
	X float32
	Y float32
}

// ---------- table schemas ----------

type Entity struct {
	Id       uint32 `stdb:"primarykey,autoinc"`
	Position Vector2
	Mass     uint32
}

type Circle struct {
	EntityId      uint32          `stdb:"primarykey"`
	PlayerId      uint32          `stdb:"index=btree"`
	Direction     Vector2
	Magnitude     float32
	LastSplitTime types.Timestamp
}

type Food struct {
	EntityId uint32 `stdb:"primarykey"`
}

// ---------- helper functions ----------

func massToRadius(mass uint32) float32 {
	return float32(math.Sqrt(float64(mass)))
}

func isOverlapping(entity1, entity2 Entity) bool {
	entity1Radius := massToRadius(entity1.Mass)
	entity2Radius := massToRadius(entity2.Mass)
	dx := entity1.Position.X - entity2.Position.X
	dy := entity1.Position.Y - entity2.Position.Y
	distance := float32(math.Sqrt(float64(dx*dx + dy*dy)))
	maxRadius := entity1Radius
	if entity2Radius > maxRadius {
		maxRadius = entity2Radius
	}
	return distance < maxRadius
}

// ---------- index names ----------

const (
	entityIdIdxBtree       = "entity_id_idx_btree"
	circleEntityIdIdxBtree = "circle_entity_id_idx_btree"
	circlePlayerIdIdxBtree = "circle_player_id_idx_btree"
	foodEntityIdIdxBtree   = "food_entity_id_idx_btree"
)

// ---------- logger ----------

var circlesLogger log.Logger

// ---------- bulk insert functions ----------

func insertBulkEntity(ctx server.ReducerContext, count uint32) {
	for i := uint32(0); i < count; i++ {
		runtime.Insert(Entity{
			Id:       0,
			Position: Vector2{X: 0, Y: 0},
			Mass:     0,
		})
	}
	circlesLogger.Info(fmt.Sprintf("INSERT ENTITY: %d", count))
}

func insertBulkCircle(ctx server.ReducerContext, count uint32) {
	for i := uint32(0); i < count; i++ {
		runtime.Insert(Circle{
			EntityId:      i,
			PlayerId:      i,
			Direction:     Vector2{X: 0, Y: 0},
			Magnitude:     0,
			LastSplitTime: types.NewTimestamp(0),
		})
	}
	circlesLogger.Info(fmt.Sprintf("INSERT CIRCLE: %d", count))
}

func insertBulkFood(ctx server.ReducerContext, count uint32) {
	for i := uint32(0); i < count; i++ {
		runtime.Insert(Food{
			EntityId: i,
		})
	}
	circlesLogger.Info(fmt.Sprintf("INSERT FOOD: %d", count))
}

// ---------- join query functions ----------

// crossJoinAll simulates: SELECT * FROM Circle, Entity, Food
func crossJoinAll(ctx server.ReducerContext, expected uint32) {
	var count uint32

	circleIter, err := runtime.Scan[Circle]()
	if err != nil {
		circlesLogger.Error(fmt.Sprintf("failed to scan circles: %v", err))
		return
	}
	defer circleIter.Close()

	for circle, ok := circleIter.Next(); ok; circle, ok = circleIter.Next() {
		_ = circle

		entityIter, err := runtime.Scan[Entity]()
		if err != nil {
			circlesLogger.Error(fmt.Sprintf("failed to scan entities: %v", err))
			return
		}

		for entity, ok := entityIter.Next(); ok; entity, ok = entityIter.Next() {
			_ = entity

			foodIter, err := runtime.Scan[Food]()
			if err != nil {
				circlesLogger.Error(fmt.Sprintf("failed to scan foods: %v", err))
				entityIter.Close()
				return
			}

			for food, ok := foodIter.Next(); ok; food, ok = foodIter.Next() {
				_ = food
				count++
			}
			foodIter.Close()
		}
		entityIter.Close()
	}

	circlesLogger.Info(fmt.Sprintf("CROSS JOIN ALL: %d, processed: %d", expected, count))
}

// crossJoinCircleFood simulates:
// SELECT * FROM Circle JOIN Entity USING(entity_id), Food JOIN Entity USING(entity_id)
func crossJoinCircleFood(ctx server.ReducerContext, expected uint32) {
	var count uint32

	circleIter, err := runtime.Scan[Circle]()
	if err != nil {
		circlesLogger.Error(fmt.Sprintf("failed to scan circles: %v", err))
		return
	}
	defer circleIter.Close()

	for circle, ok := circleIter.Next(); ok; circle, ok = circleIter.Next() {
		circleEntity, found, err := runtime.FindBy[Entity, uint32](entityIdIdxBtree, circle.EntityId)
		if err != nil {
			circlesLogger.Error(fmt.Sprintf("failed to find entity: %v", err))
			return
		}
		if !found {
			continue
		}

		foodIter, err := runtime.Scan[Food]()
		if err != nil {
			circlesLogger.Error(fmt.Sprintf("failed to scan foods: %v", err))
			return
		}

		for food, ok := foodIter.Next(); ok; food, ok = foodIter.Next() {
			count++
			foodEntity, found, err := runtime.FindBy[Entity, uint32](entityIdIdxBtree, food.EntityId)
			if err != nil {
				circlesLogger.Error(fmt.Sprintf("failed to find food entity: %v", err))
				foodIter.Close()
				return
			}
			if !found {
				circlesLogger.Error(fmt.Sprintf("Entity not found: %d", food.EntityId))
				foodIter.Close()
				return
			}
			_ = isOverlapping(circleEntity, foodEntity)
		}
		foodIter.Close()
	}

	circlesLogger.Info(fmt.Sprintf("CROSS JOIN CIRCLE FOOD: %d, processed: %d", expected, count))
}

// ---------- game init/run functions ----------

func initGameCircles(ctx server.ReducerContext, initialLoad uint32) {
	biggestTable := initialLoad * 100
	bigTable := initialLoad * 50
	smallTable := initialLoad

	insertBulkFood(ctx, biggestTable)
	insertBulkEntity(ctx, bigTable)
	insertBulkCircle(ctx, smallTable)
}

func runGameCircles(ctx server.ReducerContext, initialLoad uint32) {
	smallTable := initialLoad

	crossJoinCircleFood(ctx, smallTable)
	crossJoinAll(ctx, smallTable)
}

// ---------- registration ----------

func init() {
	circlesLogger = log.NewLogger("circles")

	// Register tables
	server.RegisterTable[Entity]("entity", server.TableAccessPublic)
	server.RegisterTable[Circle]("circle", server.TableAccessPublic)
	server.RegisterTable[Food]("food", server.TableAccessPublic)

	// Register reducers
	server.RegisterReducer("insert_bulk_entity", insertBulkEntity)
	server.RegisterReducer("insert_bulk_circle", insertBulkCircle)
	server.RegisterReducer("insert_bulk_food", insertBulkFood)
	server.RegisterReducer("cross_join_all", crossJoinAll)
	server.RegisterReducer("cross_join_circle_food", crossJoinCircleFood)
	server.RegisterReducer("init_game_circles", initGameCircles)
	server.RegisterReducer("run_game_circles", runGameCircles)
}
