namespace SpacetimeDB.Testing.Tests;

using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;
using TestingModule;

/// <summary>Builds the test module's schema for tests that drive the native datastore directly.</summary>
internal static class TestSchema
{
    public static byte[] Build()
    {
        var builder = new ModuleBuilder();
        Schema.Register(builder);
        RawModuleDef schema = new RawModuleDef.V10(builder.BuildModuleDefinition());
        return IStructuralReadWrite.ToBytes(new RawModuleDef.BSATN(), schema);
    }
}
