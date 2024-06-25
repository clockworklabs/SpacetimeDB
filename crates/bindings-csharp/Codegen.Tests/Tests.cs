namespace Codegen.Tests;

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

    public static (CSharpCompilation, CSharpCompilation) ReadSample(string name)
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
            assemblyName: name,
            references: Enumerable
                .Concat(dotNetAssemblies, stdbAssemblies)
                .Select(assemblyPath => MetadataReference.CreateFromFile(assemblyPath)),
            options: new(
                OutputKind.NetModule,
                nullableContextOptions: NullableContextOptions.Enable
            )
        );

        var sampleCode = File.ReadAllText($"{projectDir}/{name}.cs");
        var sampleCompilation = baseCompilation.AddSyntaxTrees(CSharpSyntaxTree.ParseText(sampleCode));

        // Add a comment to the end of each line to make the code modified with no functional changes.
        var modifiedCode = sampleCode.ReplaceLineEndings($"// Modified{Environment.NewLine}");
        var modifiedCompilation = baseCompilation.AddSyntaxTrees(
            CSharpSyntaxTree.ParseText(modifiedCode)
        );

        return (sampleCompilation, modifiedCompilation);
    }

    record struct StepOutput(string Key, IncrementalStepRunReason Reason, object Value);

    [Theory]
    [InlineData(typeof(SpacetimeDB.Codegen.Module), "Sample")]
    [InlineData(typeof(SpacetimeDB.Codegen.Type), "Sample")]
    [InlineData(typeof(SpacetimeDB.Codegen.Module), "SampleProblems")]
    [InlineData(typeof(SpacetimeDB.Codegen.Type), "SampleProblems")]
    public async Task VerifyDriver(Type generatorType, string sampleName)
    {
        var (sampleCompilation, modifiedCompilation) = ReadSample(sampleName);

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
        await Verify(driverAfterGen).UseDirectory($"snapshots/{sampleName}").UseFileName(generatorType.Name);

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
