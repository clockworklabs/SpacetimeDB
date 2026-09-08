namespace Runtime.Tests;

using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;

public class FunctionVisibilityTests
{
    [Theory]
    [InlineData(FunctionVisibility.Private, 0)]
    [InlineData(FunctionVisibility.ClientCallable, 1)]
    [InlineData(FunctionVisibility.Internal, 2)]
    [InlineData(FunctionVisibility.ExplicitClientCallable, 3)]
    public void V10RetainsVisibilityEnumEncoding(FunctionVisibility visibility, byte tag)
    {
        var bytes = IStructuralReadWrite.ToBytes(
            new SpacetimeDB.BSATN.Enum<FunctionVisibility>(),
            visibility
        );
        Assert.Equal(new byte[] { tag }, bytes);
    }

    [Theory]
    [InlineData(FunctionVisibility.ExplicitClientCallable)]
    [InlineData(FunctionVisibility.ClientCallable)]
    [InlineData(FunctionVisibility.Private)]
    [InlineData(FunctionVisibility.Internal)]
    public void SchedulingPreservesVisibility(FunctionVisibility visibility)
    {
        var module = new RawModuleDefV10();
        var reducer = new RawReducerDefV10(
            "run_job",
            [],
            visibility,
            AlgebraicType.Unit,
            new AlgebraicType.String(default)
        );
        module.RegisterReducer(reducer, null);
        module.RegisterTable(
            new RawTableDefV10 { SourceName = "jobs" },
            new RawScheduleDefV10(null, "jobs", 0, "run_job")
        );
        var raw = module.BuildModuleDefinition();
        var reducers = Assert.Single(raw.Sections.OfType<RawModuleDefV10Section.Reducers>());
        Assert.Equal(visibility, Assert.Single(reducers.Reducers_).Visibility);
        var capabilities = Assert.Single(
            raw.Sections.OfType<RawModuleDefV10Section.Capabilities>()
        );
        Assert.Contains("hosted_auth_v1", capabilities.Capabilities_);
    }

    [Theory]
    [InlineData(FunctionVisibility.ClientCallable)]
    [InlineData(FunctionVisibility.ExplicitClientCallable)]
    public void LifecycleRejectsExternalVisibility(FunctionVisibility visibility)
    {
        var module = new RawModuleDefV10();
        var reducer = new RawReducerDefV10(
            "initialize",
            [],
            visibility,
            AlgebraicType.Unit,
            new AlgebraicType.String(default)
        );
        Assert.Throws<InvalidOperationException>(
            () => module.RegisterReducer(reducer, Lifecycle.Init)
        );
    }
}
