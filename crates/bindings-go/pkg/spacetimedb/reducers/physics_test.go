package reducers

import (
	"math"
	"testing"
)

func TestVector2Operations(t *testing.T) {
	t.Run("NewVector2", func(t *testing.T) {
		v := NewVector2(3, 4)
		if v.X != 3 || v.Y != 4 {
			t.Errorf("Expected (3, 4), got (%f, %f)", v.X, v.Y)
		}
	})

	t.Run("ZeroVector2", func(t *testing.T) {
		v := ZeroVector2()
		if v.X != 0 || v.Y != 0 {
			t.Errorf("Expected (0, 0), got (%f, %f)", v.X, v.Y)
		}
	})

	t.Run("Add", func(t *testing.T) {
		v1 := NewVector2(1, 2)
		v2 := NewVector2(3, 4)
		result := v1.Add(v2)
		if result.X != 4 || result.Y != 6 {
			t.Errorf("Expected (4, 6), got (%f, %f)", result.X, result.Y)
		}
	})

	t.Run("Sub", func(t *testing.T) {
		v1 := NewVector2(5, 7)
		v2 := NewVector2(2, 3)
		result := v1.Sub(v2)
		if result.X != 3 || result.Y != 4 {
			t.Errorf("Expected (3, 4), got (%f, %f)", result.X, result.Y)
		}
	})

	t.Run("Mul", func(t *testing.T) {
		v := NewVector2(2, 3)
		result := v.Mul(2)
		if result.X != 4 || result.Y != 6 {
			t.Errorf("Expected (4, 6), got (%f, %f)", result.X, result.Y)
		}
	})

	t.Run("Div", func(t *testing.T) {
		v := NewVector2(4, 6)
		result := v.Div(2)
		if result.X != 2 || result.Y != 3 {
			t.Errorf("Expected (2, 3), got (%f, %f)", result.X, result.Y)
		}
	})

	t.Run("DivByZero", func(t *testing.T) {
		v := NewVector2(4, 6)
		result := v.Div(0)
		if result.X != 0 || result.Y != 0 {
			t.Errorf("Expected (0, 0) for division by zero, got (%f, %f)", result.X, result.Y)
		}
	})

	t.Run("SqrMagnitude", func(t *testing.T) {
		v := NewVector2(3, 4)
		result := v.SqrMagnitude()
		expected := float32(25) // 3^2 + 4^2 = 25
		if result != expected {
			t.Errorf("Expected %f, got %f", expected, result)
		}
	})

	t.Run("Magnitude", func(t *testing.T) {
		v := NewVector2(3, 4)
		result := v.Magnitude()
		expected := float32(5) // sqrt(25) = 5
		if result != expected {
			t.Errorf("Expected %f, got %f", expected, result)
		}
	})

	t.Run("Normalized", func(t *testing.T) {
		v := NewVector2(3, 4)
		result := v.Normalized()
		expected := NewVector2(0.6, 0.8) // (3/5, 4/5)
		if !ApproxEqual(result.X, expected.X) || !ApproxEqual(result.Y, expected.Y) {
			t.Errorf("Expected (%f, %f), got (%f, %f)", expected.X, expected.Y, result.X, result.Y)
		}
	})

	t.Run("NormalizedZero", func(t *testing.T) {
		v := ZeroVector2()
		result := v.Normalized()
		if result.X != 0 || result.Y != 0 {
			t.Errorf("Expected (0, 0) for zero vector normalization, got (%f, %f)", result.X, result.Y)
		}
	})

	t.Run("Distance", func(t *testing.T) {
		v1 := NewVector2(0, 0)
		v2 := NewVector2(3, 4)
		result := v1.Distance(v2)
		expected := float32(5)
		if result != expected {
			t.Errorf("Expected %f, got %f", expected, result)
		}
	})

	t.Run("SqrDistance", func(t *testing.T) {
		v1 := NewVector2(0, 0)
		v2 := NewVector2(3, 4)
		result := v1.SqrDistance(v2)
		expected := float32(25)
		if result != expected {
			t.Errorf("Expected %f, got %f", expected, result)
		}
	})

	t.Run("Dot", func(t *testing.T) {
		v1 := NewVector2(2, 3)
		v2 := NewVector2(4, 5)
		result := v1.Dot(v2)
		expected := float32(23) // 2*4 + 3*5 = 23
		if result != expected {
			t.Errorf("Expected %f, got %f", expected, result)
		}
	})

	t.Run("IsZero", func(t *testing.T) {
		zero := ZeroVector2()
		nonZero := NewVector2(1, 0)

		if !zero.IsZero() {
			t.Error("Zero vector should be zero")
		}
		if nonZero.IsZero() {
			t.Error("Non-zero vector should not be zero")
		}
	})
}

func TestBounds(t *testing.T) {
	t.Run("NewBounds", func(t *testing.T) {
		bounds := NewBounds(1, 2, 5, 6)
		if bounds.MinX != 1 || bounds.MinY != 2 || bounds.MaxX != 5 || bounds.MaxY != 6 {
			t.Errorf("Expected bounds (1,2,5,6), got (%f,%f,%f,%f)",
				bounds.MinX, bounds.MinY, bounds.MaxX, bounds.MaxY)
		}
	})

	t.Run("BoundsFromCircle", func(t *testing.T) {
		center := NewVector2(5, 5)
		radius := float32(2)
		bounds := BoundsFromCircle(center, radius)

		if bounds.MinX != 3 || bounds.MinY != 3 || bounds.MaxX != 7 || bounds.MaxY != 7 {
			t.Errorf("Expected bounds (3,3,7,7), got (%f,%f,%f,%f)",
				bounds.MinX, bounds.MinY, bounds.MaxX, bounds.MaxY)
		}
	})

	t.Run("BoundsFromRect", func(t *testing.T) {
		center := NewVector2(5, 5)
		width := float32(4)
		height := float32(6)
		bounds := BoundsFromRect(center, width, height)

		if bounds.MinX != 3 || bounds.MinY != 2 || bounds.MaxX != 7 || bounds.MaxY != 8 {
			t.Errorf("Expected bounds (3,2,7,8), got (%f,%f,%f,%f)",
				bounds.MinX, bounds.MinY, bounds.MaxX, bounds.MaxY)
		}
	})

	t.Run("Overlaps", func(t *testing.T) {
		bounds1 := NewBounds(0, 0, 5, 5)
		bounds2 := NewBounds(3, 3, 8, 8)   // Overlapping
		bounds3 := NewBounds(6, 6, 10, 10) // Non-overlapping

		if !bounds1.Overlaps(bounds2) {
			t.Error("bounds1 should overlap bounds2")
		}
		if bounds1.Overlaps(bounds3) {
			t.Error("bounds1 should not overlap bounds3")
		}
	})

	t.Run("Contains", func(t *testing.T) {
		bounds := NewBounds(0, 0, 5, 5)
		inside := NewVector2(2, 3)
		outside := NewVector2(6, 3)
		edge := NewVector2(5, 5)

		if !bounds.Contains(inside) {
			t.Error("Bounds should contain inside point")
		}
		if bounds.Contains(outside) {
			t.Error("Bounds should not contain outside point")
		}
		if !bounds.Contains(edge) {
			t.Error("Bounds should contain edge point")
		}
	})

	t.Run("Area", func(t *testing.T) {
		bounds := NewBounds(0, 0, 4, 3)
		expected := float32(12) // 4 * 3
		if bounds.Area() != expected {
			t.Errorf("Expected area %f, got %f", expected, bounds.Area())
		}
	})

	t.Run("Center", func(t *testing.T) {
		bounds := NewBounds(2, 4, 8, 10)
		center := bounds.Center()
		expected := NewVector2(5, 7)
		if center.X != expected.X || center.Y != expected.Y {
			t.Errorf("Expected center (%f, %f), got (%f, %f)",
				expected.X, expected.Y, center.X, center.Y)
		}
	})
}

func TestCircle(t *testing.T) {
	t.Run("NewCircle", func(t *testing.T) {
		center := NewVector2(3, 4)
		radius := float32(5)
		circle := NewCircle(center, radius)

		if circle.Center.X != 3 || circle.Center.Y != 4 || circle.Radius != 5 {
			t.Errorf("Expected circle at (3,4) with radius 5, got (%f,%f) with radius %f",
				circle.Center.X, circle.Center.Y, circle.Radius)
		}
	})

	t.Run("Overlaps", func(t *testing.T) {
		circle1 := NewCircle(NewVector2(0, 0), 3)
		circle2 := NewCircle(NewVector2(4, 0), 2) // Overlapping (distance=4, radii=3+2=5)
		circle3 := NewCircle(NewVector2(8, 0), 2) // Non-overlapping (distance=8, radii=3+2=5)

		if !circle1.Overlaps(circle2) {
			t.Error("circle1 should overlap circle2")
		}
		if circle1.Overlaps(circle3) {
			t.Error("circle1 should not overlap circle3")
		}
	})

	t.Run("OverlapsWithThreshold", func(t *testing.T) {
		circle1 := NewCircle(NewVector2(0, 0), 3)
		circle2 := NewCircle(NewVector2(4, 0), 2) // Distance=4, radii=3+2=5
		threshold := float32(0.2)                 // 20% threshold means radii sum becomes 5*0.8=4

		if !circle1.OverlapsWithThreshold(circle2, threshold) {
			t.Error("circle1 should overlap circle2 with threshold")
		}
	})

	t.Run("Contains", func(t *testing.T) {
		circle := NewCircle(NewVector2(5, 5), 3)
		inside := NewVector2(6, 6)  // Distance ~1.41 < 3
		outside := NewVector2(9, 5) // Distance 4 > 3
		edge := NewVector2(8, 5)    // Distance 3 = 3

		if !circle.Contains(inside) {
			t.Error("Circle should contain inside point")
		}
		if circle.Contains(outside) {
			t.Error("Circle should not contain outside point")
		}
		if !circle.Contains(edge) {
			t.Error("Circle should contain edge point")
		}
	})

	t.Run("Area", func(t *testing.T) {
		circle := NewCircle(NewVector2(0, 0), 2)
		expected := float32(math.Pi) * 4 // π * r²
		if !ApproxEqual(circle.Area(), expected) {
			t.Errorf("Expected area %f, got %f", expected, circle.Area())
		}
	})

	t.Run("Bounds", func(t *testing.T) {
		circle := NewCircle(NewVector2(5, 7), 3)
		bounds := circle.Bounds()

		if bounds.MinX != 2 || bounds.MinY != 4 || bounds.MaxX != 8 || bounds.MaxY != 10 {
			t.Errorf("Expected bounds (2,4,8,10), got (%f,%f,%f,%f)",
				bounds.MinX, bounds.MinY, bounds.MaxX, bounds.MaxY)
		}
	})
}

func TestRectangle(t *testing.T) {
	t.Run("NewRectangle", func(t *testing.T) {
		center := NewVector2(5, 5)
		width := float32(4)
		height := float32(6)
		rect := NewRectangle(center, width, height)

		if rect.Center.X != 5 || rect.Center.Y != 5 || rect.Width != 4 || rect.Height != 6 {
			t.Errorf("Expected rectangle at (5,5) with size 4x6, got (%f,%f) with size %fx%f",
				rect.Center.X, rect.Center.Y, rect.Width, rect.Height)
		}
	})

	t.Run("Overlaps", func(t *testing.T) {
		rect1 := NewRectangle(NewVector2(2, 2), 4, 4) // Bounds: (0,0,4,4)
		rect2 := NewRectangle(NewVector2(3, 3), 2, 2) // Bounds: (2,2,4,4) - Overlapping
		rect3 := NewRectangle(NewVector2(6, 6), 2, 2) // Bounds: (5,5,7,7) - Non-overlapping

		if !rect1.Overlaps(rect2) {
			t.Error("rect1 should overlap rect2")
		}
		if rect1.Overlaps(rect3) {
			t.Error("rect1 should not overlap rect3")
		}
	})

	t.Run("Contains", func(t *testing.T) {
		rect := NewRectangle(NewVector2(5, 5), 4, 4) // Bounds: (3,3,7,7)
		inside := NewVector2(5, 5)
		outside := NewVector2(8, 5)
		edge := NewVector2(7, 7)

		if !rect.Contains(inside) {
			t.Error("Rectangle should contain inside point")
		}
		if rect.Contains(outside) {
			t.Error("Rectangle should not contain outside point")
		}
		if !rect.Contains(edge) {
			t.Error("Rectangle should contain edge point")
		}
	})
}

func TestSpatialGrid(t *testing.T) {
	t.Run("NewSpatialGrid", func(t *testing.T) {
		grid := NewSpatialGrid(100, 100, 10)

		if grid.worldWidth != 100 || grid.worldHeight != 100 || grid.cellSize != 10 {
			t.Error("Grid dimensions not set correctly")
		}

		expectedCols := 10 // ceil(100/10)
		expectedRows := 10
		if grid.cols != expectedCols || grid.rows != expectedRows {
			t.Errorf("Expected %dx%d grid, got %dx%d", expectedCols, expectedRows, grid.cols, grid.rows)
		}
	})

	t.Run("InsertAndQuery", func(t *testing.T) {
		grid := NewSpatialGrid(100, 100, 10)

		// Insert entity in cell (0,0) - bounds (0,0,5,5)
		bounds1 := NewBounds(0, 0, 5, 5)
		grid.Insert(1, bounds1)

		// Insert entity in cell (1,1) - bounds (15,15,20,20)
		bounds2 := NewBounds(15, 15, 20, 20)
		grid.Insert(2, bounds2)

		// Query overlapping area
		queryBounds := NewBounds(0, 0, 8, 8) // Should find entity 1
		results := grid.Query(queryBounds)

		found := false
		for _, entityID := range results {
			if entityID == 1 {
				found = true
				break
			}
		}
		if !found {
			t.Error("Should find entity 1 in query results")
		}

		// Query non-overlapping area
		queryBounds2 := NewBounds(50, 50, 60, 60)
		results2 := grid.Query(queryBounds2)
		if len(results2) > 0 {
			t.Error("Should not find any entities in empty area")
		}
	})

	t.Run("Clear", func(t *testing.T) {
		grid := NewSpatialGrid(100, 100, 10)

		// Insert an entity
		bounds := NewBounds(0, 0, 5, 5)
		grid.Insert(1, bounds)

		// Clear the grid
		grid.Clear()

		// Query should return no results
		results := grid.Query(bounds)
		if len(results) > 0 {
			t.Error("Grid should be empty after clear")
		}
	})
}

func TestCollisionDetector(t *testing.T) {
	t.Run("FindPotentialCollisions", func(t *testing.T) {
		detector := NewCollisionDetector(100, 100, 10)

		entities := map[uint32]Circle{
			1: NewCircle(NewVector2(5, 5), 2),   // Should collide with 2
			2: NewCircle(NewVector2(7, 5), 2),   // Should collide with 1
			3: NewCircle(NewVector2(50, 50), 2), // Should not collide with others
		}

		pairs := detector.FindPotentialCollisions(entities)

		// Should find at least one collision pair (1,2)
		foundPair := false
		for _, pair := range pairs {
			if (pair.EntityA == 1 && pair.EntityB == 2) || (pair.EntityA == 2 && pair.EntityB == 1) {
				foundPair = true
				break
			}
		}

		if !foundPair {
			t.Error("Should find collision pair between entities 1 and 2")
		}
	})
}

func TestPhysicsHelpers(t *testing.T) {
	t.Run("ClampFloat32", func(t *testing.T) {
		result := ClampFloat32(5, 0, 10)
		if result != 5 {
			t.Errorf("Expected 5, got %f", result)
		}

		result = ClampFloat32(-5, 0, 10)
		if result != 0 {
			t.Errorf("Expected 0, got %f", result)
		}

		result = ClampFloat32(15, 0, 10)
		if result != 10 {
			t.Errorf("Expected 10, got %f", result)
		}
	})

	t.Run("LerpFloat32", func(t *testing.T) {
		result := LerpFloat32(0, 10, 0.5)
		if result != 5 {
			t.Errorf("Expected 5, got %f", result)
		}

		result = LerpFloat32(0, 10, 0)
		if result != 0 {
			t.Errorf("Expected 0, got %f", result)
		}

		result = LerpFloat32(0, 10, 1)
		if result != 10 {
			t.Errorf("Expected 10, got %f", result)
		}
	})

	t.Run("LerpVector2", func(t *testing.T) {
		v1 := NewVector2(0, 0)
		v2 := NewVector2(10, 20)
		result := LerpVector2(v1, v2, 0.5)
		expected := NewVector2(5, 10)

		if result.X != expected.X || result.Y != expected.Y {
			t.Errorf("Expected (%f, %f), got (%f, %f)", expected.X, expected.Y, result.X, result.Y)
		}
	})

	t.Run("MapRangeFloat32", func(t *testing.T) {
		// Map 5 from range [0,10] to range [0,100]
		result := MapRangeFloat32(5, 0, 10, 0, 100)
		if result != 50 {
			t.Errorf("Expected 50, got %f", result)
		}
	})

	t.Run("ApproxEqual", func(t *testing.T) {
		if !ApproxEqual(1.0, 1.0000001) {
			t.Error("Very close values should be approximately equal")
		}

		if ApproxEqual(1.0, 1.1) {
			t.Error("Different values should not be approximately equal")
		}
	})

	t.Run("ApproxZero", func(t *testing.T) {
		if !ApproxZero(0.0000001) {
			t.Error("Very small value should be approximately zero")
		}

		if ApproxZero(0.1) {
			t.Error("Large value should not be approximately zero")
		}
	})

	t.Run("SafeNormalize", func(t *testing.T) {
		v1 := NewVector2(3, 4)
		result1 := SafeNormalize(v1)
		expected1 := NewVector2(0.6, 0.8)

		if !ApproxEqual(result1.X, expected1.X) || !ApproxEqual(result1.Y, expected1.Y) {
			t.Errorf("Expected (%f, %f), got (%f, %f)", expected1.X, expected1.Y, result1.X, result1.Y)
		}

		// Test with very small vector
		v2 := NewVector2(0.0000001, 0.0000001)
		result2 := SafeNormalize(v2)
		if !result2.IsZero() {
			t.Error("Very small vector should normalize to zero")
		}
	})

	t.Run("SafeDivide", func(t *testing.T) {
		result := SafeDivide(10, 2)
		if result != 5 {
			t.Errorf("Expected 5, got %f", result)
		}

		result = SafeDivide(10, 0.0000001)
		if result != 0 {
			t.Errorf("Division by very small number should return 0, got %f", result)
		}
	})
}

// Benchmark tests
func BenchmarkVector2Operations(b *testing.B) {
	v1 := NewVector2(3, 4)
	v2 := NewVector2(5, 6)

	b.Run("Add", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			v1.Add(v2)
		}
	})

	b.Run("Magnitude", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			v1.Magnitude()
		}
	})

	b.Run("Normalized", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			v1.Normalized()
		}
	})
}

func BenchmarkCircleOverlaps(b *testing.B) {
	circle1 := NewCircle(NewVector2(0, 0), 5)
	circle2 := NewCircle(NewVector2(3, 4), 3)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		circle1.Overlaps(circle2)
	}
}

func BenchmarkSpatialGridQuery(b *testing.B) {
	grid := NewSpatialGrid(1000, 1000, 50)

	// Insert many entities
	for i := uint32(0); i < 1000; i++ {
		x := float32(i%100) * 10
		y := float32(i/100) * 10
		bounds := NewBounds(x, y, x+5, y+5)
		grid.Insert(i, bounds)
	}

	queryBounds := NewBounds(100, 100, 200, 200)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		grid.Query(queryBounds)
	}
}
