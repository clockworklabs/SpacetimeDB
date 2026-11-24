using Xunit;

namespace HandwrittenModuleBindings;

// A client can extend the row without changing its wire format or its constructor.
public partial class Person
{
    public string LocalNote = "client only";
}

public class ModuleSchemaTests
{
    [Fact]
    public void ServerRowDeclarationPreservesClientConstructorsAndSerialization()
    {
        var empty = new Person();
        Assert.Equal(string.Empty, empty.Name);
        Assert.Equal(string.Empty, empty.Address.City);
        Assert.Empty(empty.Tags);

        var person = new Person(4, "Alice", new Address("Seattle"), new() { "admin" })
        {
            LocalNote = "unsaved",
        };
        using var bytes = new MemoryStream();
        var serializer = new Person.BSATN();
        using var writer = new BinaryWriter(bytes);
        serializer.Write(writer, person);
        bytes.Position = 0;
        using var reader = new BinaryReader(bytes);
        var copy = serializer.Read(reader);
        Assert.Equal(person, copy);
        Assert.Equal("client only", copy.LocalNote);
    }

    [Fact]
    public void OnlyBodyFreeFunctionsReceiveClientStubs()
    {
        Assert.Throws<NotSupportedException>(() => Module.Add(null!, "Alice"));
        Assert.Equal("Hello, Alice", Module.Greet(null!, "Alice"));
    }
}
