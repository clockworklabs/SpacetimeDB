namespace SpacetimeDB.Testing.Tests;

using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;
using TestingModule;

/// <summary>Builds the test module's schema. Lives here because BuildModuleDefinition is internal.</summary>
internal static class TestSchema
{
    public static (byte[] Schema, NamespaceRegistry Namespaces) BuildMounted(string mount)
    {
        var child = new ModuleBuilder();
        Schema.Register(child);
        var root = new ModuleBuilder();
        root.RegisterSubmodule(mount, null, child);
        var definition = root.BuildModuleDefinition();
        // This fixture only mounts tables; environment and lifecycle declarations are root-only.
        var mounted = definition
            .Sections.OfType<RawModuleDefV10Section.Submodules>()
            .Single()
            .Submodules_.Single()
            .Module;
        mounted.Sections.RemoveAll(section =>
            section
                is RawModuleDefV10Section.Environment
                    or RawModuleDefV10Section.LifeCycleReducers
        );
        RawModuleDef schema = new RawModuleDef.V10(definition);
        return (
            IStructuralReadWrite.ToBytes(new RawModuleDef.BSATN(), schema),
            new NamespaceRegistry(
                "test-root",
                CaseConversionPolicy.SnakeCase,
                [(typeof(Entry).Assembly.FullName!, mount, null, CaseConversionPolicy.SnakeCase)]
            )
        );
    }

    public static byte[] Build()
    {
        var builder = new ModuleBuilder();
        Schema.Register(builder);
        RawModuleDef schema = new RawModuleDef.V10(builder.BuildModuleDefinition());
        return IStructuralReadWrite.ToBytes(new RawModuleDef.BSATN(), schema);
    }
}
