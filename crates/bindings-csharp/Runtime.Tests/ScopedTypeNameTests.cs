namespace Runtime.Tests;

using SpacetimeDB.Internal;

public class ScopedTypeNameTests
{
    [SpacetimeDB.Type(Name = "game_world::Color")]
    public enum NamedEnum
    {
        Red,
        Green,
    }

    public enum UnnamedEnum
    {
        A,
    }

    [Fact]
    public void SplitsNameIntoScopeAndName()
    {
        var name = ModuleBuilder.MakeScopedTypeName(typeof(int), "game_world.inner::Details");

        Assert.Equal(["game_world", "inner"], name.Scope);
        Assert.Equal("Details", name.SourceName);
    }

    [Fact]
    public void NameWithoutScope()
    {
        var name = ModuleBuilder.MakeScopedTypeName(typeof(int), "Details");

        Assert.Empty(name.Scope);
        Assert.Equal("Details", name.SourceName);
    }

    [Fact]
    public void FallsBackToTypeName()
    {
        var name = ModuleBuilder.MakeScopedTypeName(typeof(UnnamedEnum), null);

        Assert.Empty(name.Scope);
        Assert.Equal("UnnamedEnum", name.SourceName);
    }

    [Fact]
    public void PlainEnumUsesTypeAttributeName()
    {
        var types = RegisteredTypeNames(registrar =>
        {
            new SpacetimeDB.BSATN.Enum<NamedEnum>().GetAlgebraicType(registrar);
            new SpacetimeDB.BSATN.Enum<UnnamedEnum>().GetAlgebraicType(registrar);
        });

        Assert.Equal(2, types.Count);
        Assert.Equal(["game_world"], types[0].Scope);
        Assert.Equal("Color", types[0].SourceName);
        Assert.Empty(types[1].Scope);
        Assert.Equal("UnnamedEnum", types[1].SourceName);
    }

    private static List<RawScopedTypeNameV10> RegisteredTypeNames(
        Action<SpacetimeDB.BSATN.ITypeRegistrar> register
    )
    {
        var builder = new ModuleBuilder();
        register(new Module.TypeRegistrar(builder));
        return
        [
            .. builder
                .BuildModuleDefinition()
                .Sections.OfType<RawModuleDefV10Section.Types>()
                .Single()
                .Types_.Select(type => type.SourceName),
        ];
    }
}
