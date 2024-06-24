namespace SpacetimeDB.Codegen.Tests;

using System.Collections.Immutable;
using System.Runtime.CompilerServices;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

public class GeneratorSnapshotTests
{
    // Note that we can't use assembly path here because it will be put in some deep nested folder.
    // Instead, to get the test project directory, we can use the `CallerFilePath` attribute which will magically give us path to the current file.
    private static string GetProjectDir([CallerFilePath] string path = "") =>
        Path.GetDirectoryName(path)!;

    private readonly CSharpCompilation sampleCompilation;
    private readonly CSharpCompilation modifiedCompilation;

    public GeneratorSnapshotTests()
    {
        var projectDir = GetProjectDir();
        var stdbAssemblies = ImmutableArray
            .Create("BSATN.Runtime", "Runtime")
            .Select(name => $"{projectDir}/../{name}/bin/Debug/net8.0/SpacetimeDB.{name}.dll");

        var dotNetDir = Path.GetDirectoryName(typeof(object).Assembly.Location)!;
        var dotNetAssemblies = ImmutableArray
            .Create("System.Private.CoreLib", "System.Runtime")
            .Select(name => $"{dotNetDir}/{name}.dll");

        var baseCompilation = CSharpCompilation.Create(
            assemblyName: "Sample",
            references: Enumerable
                .Concat(dotNetAssemblies, stdbAssemblies)
                .Select(assemblyPath => MetadataReference.CreateFromFile(assemblyPath)),
            options: new(
                OutputKind.NetModule,
                nullableContextOptions: NullableContextOptions.Enable
            )
        );

        var sampleCode = File.ReadAllText($"{projectDir}/Sample.cs");
        sampleCompilation = baseCompilation.AddSyntaxTrees(CSharpSyntaxTree.ParseText(sampleCode));

        // Add a comment to the end of each line to make the code modified with no functional changes.
        var modifiedCode = sampleCode.ReplaceLineEndings($"// Modified{Environment.NewLine}");
        modifiedCompilation = baseCompilation.AddSyntaxTrees(
            CSharpSyntaxTree.ParseText(modifiedCode)
        );
    }

    record struct StepOutput(string Key, IncrementalStepRunReason Reason, object Value);

    [Theory]
    [InlineData(typeof(SpacetimeDB.Codegen.Module))]
    [InlineData(typeof(SpacetimeDB.Codegen.Type))]
    public async Task VerifyDriver(System.Type generatorType)
    {
        var generator = (IIncrementalGenerator)Activator.CreateInstance(generatorType)!;
        var driver = CSharpGeneratorDriver.Create(
            [generator.AsSourceGenerator()],
            driverOptions: new(
                disabledOutputs: IncrementalGeneratorOutputKind.None,
                trackIncrementalGeneratorSteps: true
            )
        );
        // Store the new driver instance - it contains the results and the cache.
        var driverAfterGen = driver.RunGenerators(sampleCompilation);

        // Verify the generated code against the snapshots.
        await Verify(driverAfterGen).UseFileName(generatorType.Name);

        // Run again with a driver containing the cache and a trivially modified code to verify that the cache is working.
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
}
