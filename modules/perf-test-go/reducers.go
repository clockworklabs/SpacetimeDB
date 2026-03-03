package main

import (
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/sys"
)

const (
	numChunks    uint64 = 1000
	rowsPerChunk uint64 = 1200
	testID       uint64 = 989987
	testChunk    uint64 = testID / rowsPerChunk
)

//stdb:reducer name=load_location_table
func loadLocationTable(_ server.ReducerContext) {
	for chunk := uint64(0); chunk < numChunks; chunk++ {
		for i := uint64(0); i < rowsPerChunk; i++ {
			id := chunk*1200 + i
			x := int32(0)
			z := int32(chunk)
			dimension := uint32(id)
			LocationTable.Insert(Location{
				Id:        id,
				Chunk:     chunk,
				X:         x,
				Z:         z,
				Dimension: dimension,
			})
		}
	}
}

//stdb:reducer name=test_index_scan_on_id
func testIndexScanOnId(_ server.ReducerContext) {
	timerId := sys.ConsoleTimerStart("Index scan on {id}")
	location, found, err := LocationTable.FindById(testID)
	_ = sys.ConsoleTimerEnd(timerId)
	if err != nil {
		panic(fmt.Sprintf("FindBy error: %v", err))
	}
	if !found {
		panic(fmt.Sprintf("location with id %d not found", testID))
	}
	if location.Id != testID {
		panic(fmt.Sprintf("expected id %d, got %d", testID, location.Id))
	}
}

//stdb:reducer name=test_index_scan_on_chunk
func testIndexScanOnChunk(_ server.ReducerContext) {
	timerId := sys.ConsoleTimerStart("Index scan on {chunk}")
	iter, err := LocationTable.FilterByChunk(testChunk)
	_ = sys.ConsoleTimerEnd(timerId)
	if err != nil {
		panic(fmt.Sprintf("FilterBy error: %v", err))
	}
	count := uint64(0)
	for {
		_, ok := iter.Next()
		if !ok {
			break
		}
		count++
	}
	if count != rowsPerChunk {
		panic(fmt.Sprintf("expected %d rows, got %d", rowsPerChunk, count))
	}
}

//stdb:reducer name=test_index_scan_on_x_z_dimension
func testIndexScanOnXZDimension(_ server.ReducerContext) {
	z := int32(testChunk)
	dimension := uint32(testID)
	timerId := sys.ConsoleTimerStart("Index scan on {x, z, dimension}")
	iter, err := LocationTable.FilterByXAndZAndDimension(int32(0), z, dimension)
	_ = sys.ConsoleTimerEnd(timerId)
	if err != nil {
		panic(fmt.Sprintf("FilterByMultiColumn error: %v", err))
	}
	count := 0
	for {
		_, ok := iter.Next()
		if !ok {
			break
		}
		count++
	}
	if count != 1 {
		panic(fmt.Sprintf("expected 1 row, got %d", count))
	}
}

//stdb:reducer name=test_index_scan_on_x_z
func testIndexScanOnXZ(_ server.ReducerContext) {
	z := int32(testChunk)
	timerId := sys.ConsoleTimerStart("Index scan on {x, z}")
	iter, err := LocationTable.FilterByXAndZ(int32(0), z)
	_ = sys.ConsoleTimerEnd(timerId)
	if err != nil {
		panic(fmt.Sprintf("FilterByMultiColumn error: %v", err))
	}
	count := uint64(0)
	for {
		_, ok := iter.Next()
		if !ok {
			break
		}
		count++
	}
	if count != rowsPerChunk {
		panic(fmt.Sprintf("expected %d rows, got %d", rowsPerChunk, count))
	}
}
