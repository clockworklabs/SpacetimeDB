namespace Runtime.Tests;

using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;

public class FunctionVisibilityTests
{
    [Theory]
    [InlineData(null)]
    [InlineData(FunctionVisibilityV11.ClientCallable)]
    [InlineData(FunctionVisibilityV11.Private)]
    [InlineData(FunctionVisibilityV11.Internal)]
    public void SchedulingPreservesDeclaredVisibility(FunctionVisibilityV11? visibility)
    {
        var module = new RawModuleDefV11();
        var reducer = new RawReducerDefV11(
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
        var reducers = Assert.Single(raw.Sections.OfType<RawModuleDefV11Section.Reducers>());
        Assert.Equal(visibility, Assert.Single(reducers.Reducers_).DeclaredVisibility);
        var capabilities = Assert.Single(
            raw.Sections.OfType<RawModuleDefV11Section.Capabilities>()
        );
        Assert.Contains("hosted_auth_v1", capabilities.Capabilities_);
    }

    [Theory]
    [InlineData(FunctionVisibilityV11.ClientCallable)]
    [InlineData(FunctionVisibilityV11.Private)]
    public void LifecycleRejectsExternalVisibility(FunctionVisibilityV11 visibility)
    {
        var module = new RawModuleDefV11();
        var reducer = new RawReducerDefV11(
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
