namespace SpacetimeDB.Codegen.Tests;

using System.Collections.Immutable;
using System.Runtime.CompilerServices;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.MSBuild;
using Microsoft.CodeAnalysis.Text;

public static class GeneratorSnapshotTests
{
    // Note that we can't use assembly path here because it will be put in some deep nested folder.
    // Instead, to get the test project directory, we can use the `CallerFilePath` attribute which will magically give us path to the current file.
    static string GetProjectDir([CallerFilePath] string path = "") => Path.GetDirectoryName(path)!;

    record struct StepOutput(string Key, IncrementalStepRunReason Reason, object Value);

    static async Task<CSharpCompilation> CompileFixture(string name)
    {
        using var workspace = MSBuildWorkspace.Create();
        var sampleProject = await workspace.OpenProjectAsync(
            $"{GetProjectDir()}/fixtures/{name}/{name}.csproj"
        );
        var compilation = await sampleProject.GetCompilationAsync();
        return (CSharpCompilation)compilation!;
    }

    static async Task<IEnumerable<SyntaxTree>> RunAndCheckGenerator<G>(
        CSharpCompilation sampleCompilation
    )
        where G : IIncrementalGenerator, new()
    {
        var driver = CSharpGeneratorDriver.Create(
            [new G().AsSourceGenerator()],
            driverOptions: new(
                disabledOutputs: IncrementalGeneratorOutputKind.None,
                trackIncrementalGeneratorSteps: true
            ),
            // Make sure that generated files are parsed with the same language version.
            parseOptions: new(sampleCompilation.LanguageVersion)
        );

        // Store the new driver instance - it contains the results and the cache.
        var driverAfterGen = driver.RunGenerators(sampleCompilation);

        // Verify the generated code against the snapshots.
        await Verify(driverAfterGen)
            .UseDirectory($"{GetProjectDir()}/fixtures/{sampleCompilation.AssemblyName}/snapshots")
            .UseFileName(typeof(G).Name);

        // Run again with a driver containing the cache and a trivially modified code to verify that the cache is working.
        var modifiedCompilation = sampleCompilation
            .RemoveAllSyntaxTrees()
            .AddSyntaxTrees(
                sampleCompilation.SyntaxTrees.Select(tree =>
                    tree.WithChangedText(
                        SourceText.From(
                            string.Join(
                                "\n",
                                tree.GetText().Lines.Select(line => $"{line} // Modified")
                            )
                        )
                    )
                )
            );
        var driverAfterRegen = driverAfterGen.RunGenerators(modifiedCompilation);

        var regenSteps = driverAfterRegen
            .GetRunResult()
            .Results.SelectMany(result => result.TrackedSteps)
            .Where(step => step.Key.StartsWith("SpacetimeDB."))
            .SelectMany(step =>
                step.Value.SelectMany(value => value.Outputs)
                    .Select(output => new StepOutput(step.Key, output.Reason, output.Value))
            )
            .ToImmutableArray();

        // Ensure that we have tracked steps at all.
        Assert.NotEmpty(regenSteps);

        // Ensure that all steps were cached.
        Assert.Empty(
            regenSteps.Where(step =>
                step.Reason
                    is not (IncrementalStepRunReason.Cached or IncrementalStepRunReason.Unchanged)
            )
        );

        return driverAfterGen.GetRunResult().GeneratedTrees;
    }

    static void AssertCompilationSuccessful(Compilation compilation)
    {
        var emitResult = compilation.Emit(Stream.Null);

        Assert.True(
            emitResult.Success,
            string.Join(
                "\n",
                emitResult.Diagnostics.Select(diag =>
                    CSharpDiagnosticFormatter.Instance.Format(diag)
                )
            )
        );
    }

    [Fact]
    public static async Task TypeGeneratorOnClient()
    {
        var sampleCompilation = await CompileFixture("client");

        var genOutputs = await RunAndCheckGenerator<SpacetimeDB.Codegen.Type>(sampleCompilation);

        var compilationAfterGen = sampleCompilation.AddSyntaxTrees(genOutputs);

        AssertCompilationSuccessful(compilationAfterGen);
    }

    [Fact]
    public static async Task TypeAndModuleGeneratorsOnServer()
    {
        var sampleCompilation = await CompileFixture("server");

        var genOutputs = (
            await Task.WhenAll(
                RunAndCheckGenerator<SpacetimeDB.Codegen.Type>(sampleCompilation),
                RunAndCheckGenerator<SpacetimeDB.Codegen.Module>(sampleCompilation)
            )
        ).SelectMany(output => output);

        var compilationAfterGen = sampleCompilation.AddSyntaxTrees(genOutputs);

        AssertCompilationSuccessful(compilationAfterGen);
    }
}
