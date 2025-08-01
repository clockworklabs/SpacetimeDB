> ⚠️ **Internal Project** ⚠️
>
> This project is intended for internal use only. It is **not** stable and may change without notice.

## Internal documentation

This project contains Roslyn [incremental source generators](https://github.com/dotnet/roslyn/blob/main/docs/features/incremental-generators.md) that augment types with methods for self-describing and serialization. It relies on the [BSATN.Runtime](../BSATN.Runtime/) library in the generated code.

This project provides `[SpacetimeDB.Type]`. This attribute makes types self-describing, allowing them to automatically register their structure with SpacetimeDB. It also generates serialization code to the [BSATN format](https://spacetimedb.com/docs/bsatn). Any C# type annotated with `[SpacetimeDB.Type]` can be used as a table column or reducer argument.

Any `[SpacetimeDB.Type]` must be marked `partial` to allow the generated code to add new functionality.

`[SpacetimeDB.Type]` also supports emulation of tagged enums in C#. For that, the struct needs to inherit a marker interface `SpacetimeDB.TaggedEnum<Variants>` where `Variants` is a named tuple of all possible variants, e.g.:

```csharp
[SpacetimeDB.Type]
partial record Option<T> : SpacetimeDB.TaggedEnum<(T Some, Unit None)>;
```

will generate inherited records `Option.Some(T Some_)` and `Option.None(Unit None_)`. It allows
you to use tagged enums in C# in a similar way to Rust enums by leveraging C# pattern-matching
on any instance of `Option<T>`.

## What is generated

See [`../Codegen.Tests/fixtures/client/snapshots`](../Codegen.Tests/fixtures/client/snapshots/) for examples of the generated code.
[`../Codegen.Tests/fixtures/server/snapshots`](../Codegen.Tests/fixtures/server/snapshots/) also has examples, those filenames starting with `Type#`.
In addition, in any project using this library, you can set `<EmitCompilerGeneratedFiles>true</EmitCompilerGeneratedFiles>` in the `<PropertyGroup>` of your `.csproj` to see exactly what code is geing generated for your project.

`[SpacetimeDB.Type]` automatically generates correct `Equals`, `GetHashCode`, and `ToString` methods for the type. It also generates serialization code.

Any `[SpacetimeDB.Type]` will have an auto-generated member struct named `BSATN`. This struct is zero-sized and implements the interface `SpacetimeDB.BSATN.IReadWrite<T>` interface. This is used to serialize and deserialize elements of the struct. 

```csharp
[SpacetimeDB.Type]
partial struct Banana {
    public int Freshness;
    public int LengthMeters;
}

void Example(System.IO.BinaryReader reader, System.IO.BinaryWriter writer) {
    Banana.BSATN serializer = new();
    Banana banana1 = serializer.Read(reader); // read a BSATN-encoded Banana from the reader.
    Banana banana2 = serializer.Read(reader);
    Console.Log($"bananas: {banana1} {banana2}");
    Console.Log($"equal?: {banana1.Equals(banana2)}");
    serializer.write(writer, banana2); // write a BSATN-encoded Banana to the writer.
    serializer.write(writer, banana1);
}
```

Since `Banana.BSATN` takes up no space in memory, allocating one is free. We use this pattern because the C# versions we target don't support static interface methods.

`[SpacetimeDB.Type]`s that do not inherit from `SpacetimeDB.TaggedEnum` implement an additional interface, `IStructuralReadWrite`. This allows them to be read without using a serializer. (This is not possible for `TaggedEnum`s because their concrete type is not known before deserialization.)

```csharp
void Example(System.IO.BinaryReader reader, System.IO.BinaryWriter writer) {
    Banana banana = new(); // has default field values.
    banana.ReadFields(reader); // now it is initialized.
    banana.WriteFields(writer); // and we can write it out directly as well.
}
```

The `IReadWrite` interface has an additional method, `AlgebraicType GetAlgebraicType()`. This returns a description of the type that is used during module initialization; see [`../Runtime`](../Runtime/) for more information.

## Testing
The testing for this project lives in two places.
- [`../Codegen.Tests`](../Codegen.Tests/) contains snapshot-based tests. These verify that the generated code looks as expected and allow it to be reviewed more easily.
- Randomized runtime tests live in [`../BSATN.Runtime.Tests`](../BSATN.Runtime.Tests/). These tests randomly fuzz the generated serializers for a variety of types.