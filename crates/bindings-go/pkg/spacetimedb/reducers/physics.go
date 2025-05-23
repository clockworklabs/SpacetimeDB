// Package reducers - Universal physics utilities for SpacetimeDB games
package reducers

import (
	"math"
)

// Vector2 represents a universal 2D vector for physics calculations
type Vector2 struct {
	X, Y float32
}

// NewVector2 creates a new Vector2
func NewVector2(x, y float32) Vector2 {
	return Vector2{X: x, Y: y}
}

// Zero returns a zero vector
func ZeroVector2() Vector2 {
	return Vector2{X: 0, Y: 0}
}

// Vector2 mathematical operations

// Add adds two vectors
func (v Vector2) Add(other Vector2) Vector2 {
	return Vector2{X: v.X + other.X, Y: v.Y + other.Y}
}

// Sub subtracts two vectors
func (v Vector2) Sub(other Vector2) Vector2 {
	return Vector2{X: v.X - other.X, Y: v.Y - other.Y}
}

// Mul multiplies vector by scalar
func (v Vector2) Mul(scalar float32) Vector2 {
	return Vector2{X: v.X * scalar, Y: v.Y * scalar}
}

// Div divides vector by scalar
func (v Vector2) Div(scalar float32) Vector2 {
	if scalar == 0 {
		return ZeroVector2()
	}
	return Vector2{X: v.X / scalar, Y: v.Y / scalar}
}

// SqrMagnitude returns the squared magnitude of the vector
func (v Vector2) SqrMagnitude() float32 {
	return v.X*v.X + v.Y*v.Y
}

// Magnitude returns the magnitude of the vector
func (v Vector2) Magnitude() float32 {
	return float32(math.Sqrt(float64(v.SqrMagnitude())))
}

// Normalized returns a normalized version of the vector
func (v Vector2) Normalized() Vector2 {
	mag := v.Magnitude()
	if mag == 0 {
		return ZeroVector2()
	}
	return v.Div(mag)
}

// Distance returns the distance between two points
func (v Vector2) Distance(other Vector2) float32 {
	return v.Sub(other).Magnitude()
}

// SqrDistance returns the squared distance between two points
func (v Vector2) SqrDistance(other Vector2) float32 {
	return v.Sub(other).SqrMagnitude()
}

// Dot returns the dot product of two vectors
func (v Vector2) Dot(other Vector2) float32 {
	return v.X*other.X + v.Y*other.Y
}

// IsZero checks if the vector is zero
func (v Vector2) IsZero() bool {
	return v.X == 0 && v.Y == 0
}

// Bounds represents a 2D bounding box for collision detection
type Bounds struct {
	MinX, MinY, MaxX, MaxY float32
}

// NewBounds creates a new bounding box
func NewBounds(minX, minY, maxX, maxY float32) Bounds {
	return Bounds{MinX: minX, MinY: minY, MaxX: maxX, MaxY: maxY}
}

// BoundsFromCircle creates a bounding box for a circle
func BoundsFromCircle(center Vector2, radius float32) Bounds {
	return Bounds{
		MinX: center.X - radius,
		MinY: center.Y - radius,
		MaxX: center.X + radius,
		MaxY: center.Y + radius,
	}
}

// BoundsFromRect creates a bounding box for a rectangle
func BoundsFromRect(center Vector2, width, height float32) Bounds {
	halfWidth := width / 2
	halfHeight := height / 2
	return Bounds{
		MinX: center.X - halfWidth,
		MinY: center.Y - halfHeight,
		MaxX: center.X + halfWidth,
		MaxY: center.Y + halfHeight,
	}
}

// Overlaps checks if two bounding boxes overlap
func (b Bounds) Overlaps(other Bounds) bool {
	return b.MinX <= other.MaxX && b.MaxX >= other.MinX &&
		b.MinY <= other.MaxY && b.MaxY >= other.MinY
}

// Contains checks if a point is inside the bounding box
func (b Bounds) Contains(point Vector2) bool {
	return point.X >= b.MinX && point.X <= b.MaxX &&
		point.Y >= b.MinY && point.Y <= b.MaxY
}

// Area returns the area of the bounding box
func (b Bounds) Area() float32 {
	return (b.MaxX - b.MinX) * (b.MaxY - b.MinY)
}

// Center returns the center point of the bounding box
func (b Bounds) Center() Vector2 {
	return Vector2{
		X: (b.MinX + b.MaxX) / 2,
		Y: (b.MinY + b.MaxY) / 2,
	}
}

// Circle represents a circle for collision detection
type Circle struct {
	Center Vector2
	Radius float32
}

// NewCircle creates a new circle
func NewCircle(center Vector2, radius float32) Circle {
	return Circle{Center: center, Radius: radius}
}

// Overlaps checks if two circles overlap
func (c Circle) Overlaps(other Circle) bool {
	distance := c.Center.Distance(other.Center)
	return distance <= (c.Radius + other.Radius)
}

// OverlapsWithThreshold checks if two circles overlap with a threshold
func (c Circle) OverlapsWithThreshold(other Circle, threshold float32) bool {
	distance := c.Center.Distance(other.Center)
	radiusSum := (c.Radius + other.Radius) * (1.0 - threshold)
	return distance <= radiusSum
}

// Contains checks if a point is inside the circle
func (c Circle) Contains(point Vector2) bool {
	return c.Center.Distance(point) <= c.Radius
}

// Area returns the area of the circle
func (c Circle) Area() float32 {
	return float32(math.Pi) * c.Radius * c.Radius
}

// Bounds returns the bounding box of the circle
func (c Circle) Bounds() Bounds {
	return BoundsFromCircle(c.Center, c.Radius)
}

// Rectangle represents a rectangle for collision detection
type Rectangle struct {
	Center Vector2
	Width  float32
	Height float32
}

// NewRectangle creates a new rectangle
func NewRectangle(center Vector2, width, height float32) Rectangle {
	return Rectangle{Center: center, Width: width, Height: height}
}

// Overlaps checks if two rectangles overlap
func (r Rectangle) Overlaps(other Rectangle) bool {
	return r.Bounds().Overlaps(other.Bounds())
}

// Contains checks if a point is inside the rectangle
func (r Rectangle) Contains(point Vector2) bool {
	return r.Bounds().Contains(point)
}

// Bounds returns the bounding box of the rectangle
func (r Rectangle) Bounds() Bounds {
	return BoundsFromRect(r.Center, r.Width, r.Height)
}

// CollisionDetector provides optimized collision detection utilities
type CollisionDetector struct {
	spatialGrid *SpatialGrid
}

// NewCollisionDetector creates a new collision detector
func NewCollisionDetector(worldWidth, worldHeight float32, cellSize float32) *CollisionDetector {
	return &CollisionDetector{
		spatialGrid: NewSpatialGrid(worldWidth, worldHeight, cellSize),
	}
}

// SpatialGrid provides spatial partitioning for efficient collision detection
type SpatialGrid struct {
	cellSize    float32
	worldWidth  float32
	worldHeight float32
	cols        int
	rows        int
	cells       [][]uint32 // Each cell contains entity IDs
}

// NewSpatialGrid creates a new spatial grid
func NewSpatialGrid(worldWidth, worldHeight, cellSize float32) *SpatialGrid {
	cols := int(math.Ceil(float64(worldWidth / cellSize)))
	rows := int(math.Ceil(float64(worldHeight / cellSize)))
	cells := make([][]uint32, cols*rows)
	for i := range cells {
		cells[i] = make([]uint32, 0, 4) // Pre-allocate for 4 entities per cell
	}

	return &SpatialGrid{
		cellSize:    cellSize,
		worldWidth:  worldWidth,
		worldHeight: worldHeight,
		cols:        cols,
		rows:        rows,
		cells:       cells,
	}
}

// Clear removes all entities from the spatial grid
func (sg *SpatialGrid) Clear() {
	for i := range sg.cells {
		sg.cells[i] = sg.cells[i][:0] // Clear slice but keep capacity
	}
}

// Insert adds an entity to the spatial grid
func (sg *SpatialGrid) Insert(entityID uint32, bounds Bounds) {
	minCellX := int(bounds.MinX / sg.cellSize)
	minCellY := int(bounds.MinY / sg.cellSize)
	maxCellX := int(bounds.MaxX / sg.cellSize)
	maxCellY := int(bounds.MaxY / sg.cellSize)

	// Clamp to grid bounds
	minCellX = ClampInt(minCellX, 0, sg.cols-1)
	minCellY = ClampInt(minCellY, 0, sg.rows-1)
	maxCellX = ClampInt(maxCellX, 0, sg.cols-1)
	maxCellY = ClampInt(maxCellY, 0, sg.rows-1)

	// Insert into all overlapping cells
	for y := minCellY; y <= maxCellY; y++ {
		for x := minCellX; x <= maxCellX; x++ {
			cellIndex := y*sg.cols + x
			if cellIndex >= 0 && cellIndex < len(sg.cells) {
				sg.cells[cellIndex] = append(sg.cells[cellIndex], entityID)
			}
		}
	}
}

// Query returns all entities in cells that overlap with the given bounds
func (sg *SpatialGrid) Query(bounds Bounds) []uint32 {
	minCellX := int(bounds.MinX / sg.cellSize)
	minCellY := int(bounds.MinY / sg.cellSize)
	maxCellX := int(bounds.MaxX / sg.cellSize)
	maxCellY := int(bounds.MaxY / sg.cellSize)

	// Clamp to grid bounds
	minCellX = ClampInt(minCellX, 0, sg.cols-1)
	minCellY = ClampInt(minCellY, 0, sg.rows-1)
	maxCellX = ClampInt(maxCellX, 0, sg.cols-1)
	maxCellY = ClampInt(maxCellY, 0, sg.rows-1)

	// Collect all entities from overlapping cells
	var result []uint32
	entitySet := make(map[uint32]bool) // Avoid duplicates

	for y := minCellY; y <= maxCellY; y++ {
		for x := minCellX; x <= maxCellX; x++ {
			cellIndex := y*sg.cols + x
			if cellIndex >= 0 && cellIndex < len(sg.cells) {
				for _, entityID := range sg.cells[cellIndex] {
					if !entitySet[entityID] {
						entitySet[entityID] = true
						result = append(result, entityID)
					}
				}
			}
		}
	}

	return result
}

// GetCellCount returns the number of cells in the grid
func (sg *SpatialGrid) GetCellCount() int {
	return len(sg.cells)
}

// GetEntitiesInCell returns entities in a specific cell
func (sg *SpatialGrid) GetEntitiesInCell(cellX, cellY int) []uint32 {
	if cellX < 0 || cellX >= sg.cols || cellY < 0 || cellY >= sg.rows {
		return nil
	}
	cellIndex := cellY*sg.cols + cellX
	return sg.cells[cellIndex]
}

// CollisionPair represents a pair of entities that might be colliding
type CollisionPair struct {
	EntityA uint32
	EntityB uint32
}

// FindPotentialCollisions uses spatial partitioning to find potential collision pairs
func (cd *CollisionDetector) FindPotentialCollisions(entities map[uint32]Circle) []CollisionPair {
	cd.spatialGrid.Clear()

	// Insert all entities into spatial grid
	for entityID, circle := range entities {
		bounds := circle.Bounds()
		cd.spatialGrid.Insert(entityID, bounds)
	}

	var pairs []CollisionPair
	processed := make(map[uint64]bool) // Track processed pairs

	// Query for each entity and find collision pairs
	for entityID, circle := range entities {
		bounds := circle.Bounds()
		candidates := cd.spatialGrid.Query(bounds)

		for _, candidateID := range candidates {
			if candidateID != entityID {
				// Create unique pair ID (smaller ID first)
				var pairID uint64
				if entityID < candidateID {
					pairID = (uint64(entityID) << 32) | uint64(candidateID)
				} else {
					pairID = (uint64(candidateID) << 32) | uint64(entityID)
				}

				// Skip if already processed
				if processed[pairID] {
					continue
				}
				processed[pairID] = true

				pairs = append(pairs, CollisionPair{
					EntityA: entityID,
					EntityB: candidateID,
				})
			}
		}
	}

	return pairs
}

// BroadPhaseCollisionFilter filters entities using bounding box tests
func BroadPhaseCollisionFilter(entity Circle, candidates []Circle) []Circle {
	entityBounds := entity.Bounds()
	var filtered []Circle

	for _, candidate := range candidates {
		candidateBounds := candidate.Bounds()
		if entityBounds.Overlaps(candidateBounds) {
			filtered = append(filtered, candidate)
		}
	}

	return filtered
}

// Physics helper functions

// ClampFloat32 clamps a float32 value between min and max
func ClampFloat32(value, min, max float32) float32 {
	if value < min {
		return min
	}
	if value > max {
		return max
	}
	return value
}

// LerpFloat32 performs linear interpolation between two float32 values
func LerpFloat32(a, b, t float32) float32 {
	return a + t*(b-a)
}

// LerpVector2 performs linear interpolation between two Vector2 values
func LerpVector2(a, b Vector2, t float32) Vector2 {
	return Vector2{
		X: LerpFloat32(a.X, b.X, t),
		Y: LerpFloat32(a.Y, b.Y, t),
	}
}

// MapRangeFloat32 maps a value from one range to another
func MapRangeFloat32(value, fromMin, fromMax, toMin, toMax float32) float32 {
	return toMin + (value-fromMin)*(toMax-toMin)/(fromMax-fromMin)
}

// Physics constants for common calculations
const (
	Epsilon = 1e-6 // Small value for floating point comparisons
)

// ApproxEqual checks if two float32 values are approximately equal
func ApproxEqual(a, b float32) bool {
	return math.Abs(float64(a-b)) < Epsilon
}

// ApproxZero checks if a float32 value is approximately zero
func ApproxZero(value float32) bool {
	return math.Abs(float64(value)) < Epsilon
}

// SafeNormalize safely normalizes a vector, returning zero vector if magnitude is too small
func SafeNormalize(v Vector2) Vector2 {
	mag := v.Magnitude()
	if mag < Epsilon {
		return ZeroVector2()
	}
	return v.Div(mag)
}

// SafeDivide safely divides by a scalar, returning zero if divisor is too small
func SafeDivide(value, divisor float32) float32 {
	if math.Abs(float64(divisor)) < Epsilon {
		return 0
	}
	return value / divisor
}
