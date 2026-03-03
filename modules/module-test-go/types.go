package main

// TestB is a product type used in the test reducer.
type TestB struct {
	Foo string
}

// TestC is a simple enum with namespace scope "Namespace.TestC".
//stdb:enum variants=Foo,Bar scope=Namespace
type TestC uint8

const (
	TestCFoo TestC = 0
	TestCBar TestC = 1
)

// Baz is a product type used in the Foobar sum type.
type Baz struct {
	Field string
}

// Foobar is a sum type: Baz(Baz), Bar, Har(u32).
//stdb:sumtype
type Foobar interface {
	foobarTag() uint8
}

//stdb:variant of=Foobar name=Baz
type FoobarBaz struct {
	Value Baz
}

func (FoobarBaz) foobarTag() uint8 { return 0 }

//stdb:variant of=Foobar name=Bar
type FoobarBar struct{}

func (FoobarBar) foobarTag() uint8 { return 1 }

//stdb:variant of=Foobar name=Har
type FoobarHar struct {
	Value uint32
}

func (FoobarHar) foobarTag() uint8 { return 2 }

// TestF is a sum type with namespace scope "Namespace.TestF": Foo, Bar, Baz(String).
//stdb:sumtype scope=Namespace
type TestF interface {
	testFTag() uint8
}

//stdb:variant of=TestF name=Foo
type TestFFoo struct{}

func (TestFFoo) testFTag() uint8 { return 0 }

//stdb:variant of=TestF name=Bar
type TestFBar struct{}

func (TestFBar) testFTag() uint8 { return 1 }

//stdb:variant of=TestF name=Baz
type TestFBaz struct {
	Value string
}

func (TestFBaz) testFTag() uint8 { return 2 }

// TestAlias is an alias for TestA.
type TestAlias = TestA
