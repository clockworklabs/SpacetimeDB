package main

// compile using: tinygo build -o add.wasm -target wasm ./add.go

func main() {}

//export add
func add(x int, y int) int {
	return x + y
}
