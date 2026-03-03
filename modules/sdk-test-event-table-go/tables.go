package main

//stdb:table name=test_event access=public event=true
type TestEvent struct {
	Name  string
	Value uint64
}
