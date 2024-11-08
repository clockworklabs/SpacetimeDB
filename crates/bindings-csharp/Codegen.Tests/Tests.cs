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

    class Fixture(string projectDir, CSharpCompilation sampleCompilation)
    {
        public static async Task<Fixture> Compile(string name)
        {
            var projectDir = Path.Combine(GetProjectDir(), "fixtures", name);
            using var workspace = MSBuildWorkspace.Create();
            var sampleProject = await workspace.OpenProjectAsync($"{projectDir}/{name}.csproj");
            var compilation = await sampleProject.GetCompilationAsync();
            return new(projectDir, (CSharpCompilation)compilation!);
        }

        private static CSharpGeneratorDriver CreateDriver(
            IIncrementalGenerator generator,
            LanguageVersion languageVersion
        )
        {
            return CSharpGeneratorDriver.Create(
                [generator.AsSourceGenerator()],
                driverOptions: new(
                    disabledOutputs: IncrementalGeneratorOutputKind.None,
                    trackIncrementalGeneratorSteps: true
                ),
                // Make sure that generated files are parsed with the same language version.
                parseOptions: new(languageVersion)
            );
        }

        public Task Verify(string fileName, object target) =>
            Verifier.Verify(target).UseDirectory($"{projectDir}/snapshots").UseFileName(fileName);

        private async Task<IEnumerable<SyntaxTree>> RunAndCheckGenerator(
            IIncrementalGenerator generator
        )
        {
            var driver = CreateDriver(generator, sampleCompilation.LanguageVersion);

            // Store the new driver instance - it contains the results and the cache.
            var driverAfterGen = driver.RunGenerators(sampleCompilation);
            var genResult = driverAfterGen.GetRunResult();

            // Verify the generated code against the snapshots.
            await Verify(generator.GetType().Name, genResult);

            CheckCacheWorking(sampleCompilation, driverAfterGen);

            return genResult.GeneratedTrees;
        }

        public async Task<CSharpCompilation> RunAndCheckGenerators(
            params IIncrementalGenerator[] generators
        ) =>
            sampleCompilation.AddSyntaxTrees(
                (await Task.WhenAll(generators.Select(RunAndCheckGenerator))).SelectMany(output =>
                    output
                )
            );
    }

    private static void CheckCacheWorking(
        CSharpCompilation sampleCompilation,
        GeneratorDriver driverAfterGen
    )
    {
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
    }

    static IEnumerable<Diagnostic> GetCompilationErrors(Compilation compilation)
    {
        return compilation
            .Emit(Stream.Null)
            .Diagnostics.Where(diag => diag.Severity != DiagnosticSeverity.Hidden)
            // The order of diagnostics is not predictable, sort them by location to make the test deterministic.
            .OrderBy(diag => diag.Location.ToString());
    }

    [Fact]
    public static async Task TypeGeneratorOnClient()
    {
        var fixture = await Fixture.Compile("client");

        var compilationAfterGen = await fixture.RunAndCheckGenerators(
            new SpacetimeDB.Codegen.Type()
        );

        Assert.Empty(GetCompilationErrors(compilationAfterGen));
    }

    [Fact]
    public static async Task TypeAndModuleGeneratorsOnServer()
    {
        var fixture = await Fixture.Compile("server");

        var compilationAfterGen = await fixture.RunAndCheckGenerators(
            new SpacetimeDB.Codegen.Type(),
            new SpacetimeDB.Codegen.Module()
        );

        Assert.Empty(GetCompilationErrors(compilationAfterGen));
    }

    [Fact]
    public static async Task TestDiagnostics()
    {
        var fixture = await Fixture.Compile("diag");

        var compilationAfterGen = await fixture.RunAndCheckGenerators(
            new SpacetimeDB.Codegen.Type(),
            new SpacetimeDB.Codegen.Module()
        );

        // Unlike in regular tests, we don't expect this compilation to succeed - it's supposed to be full of errors.
        // We already reported the useful ones from the generator, but let's snapshot those emitted by the compiler as well.
        // This way we can notice when they get particularly noisy and improve our codegen for the case of a broken code.
        await fixture.Verify("ExtraCompilationErrors", GetCompilationErrors(compilationAfterGen));
    }
}
