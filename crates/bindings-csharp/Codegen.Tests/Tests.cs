namespace Codegen.Tests;

using System.Collections.Immutable;
using System.Linq;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Threading.Tasks;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Text;
using VerifyTests;
using Xunit;

public static class GeneratorSnapshotTests
{
    // Note that we can't use assembly path here because it will be put in some deep nested folder.
    // Instead, to get the test project directory, we can use the `CallerFilePath` attribute which will magically give us path to the current file.
    private static string GetProjectDir([CallerFilePath] string path = "") =>
        Path.GetDirectoryName(path)!;

    private static readonly CSharpCompilation SampleCompilation;

    static GeneratorSnapshotTests()
    {
        // Default diff order is weird and causes new lines to look like deleted and old as inserted.
        Environment.SetEnvironmentVariable("DiffEngine_TargetOnLeft", "true");
        // Store snapshots in a separate directory.
        UseProjectRelativeDirectory("snapshots");
        VerifySourceGenerators.Initialize();
        // Format code for more readable snapshots and to avoid diffs on whitespace changes.
        VerifierSettings.AddScrubber(
            "cs",
            (sb) =>
            {
                var unformattedCode = sb.ToString();
                sb.Clear();
                var result = CSharpier.CodeFormatter.Format(
                    unformattedCode,
                    new() { IncludeGenerated = true, EndOfLine = CSharpier.EndOfLine.LF }
                );
                if (result.CompilationErrors.Any())
                {
                    sb.AppendLine("// Generated code produced compilation errors:");
                    foreach (var diag in result.CompilationErrors)
                    {
                        sb.Append("// ").AppendLine(diag.ToString());
                    }
                    sb.AppendLine();
                }
                sb.Append(result.Code);
            },
            ScrubberLocation.Last
        );

        var projectDir = GetProjectDir();
        using var sampleSource = File.OpenRead($"{projectDir}/Sample.cs");

        var stdbAssemblies = ImmutableArray
            .Create("BSATN.Runtime", "Runtime")
            .Select(name => $"{projectDir}/../{name}/bin/Debug/net8.0/SpacetimeDB.{name}.dll");

        var dotNetDir = Path.GetDirectoryName(typeof(object).Assembly.Location)!;
        var dotNetAssemblies = ImmutableArray
            .Create("System.Private.CoreLib", "System.Runtime")
            .Select(name => $"{dotNetDir}/{name}.dll");

        SampleCompilation = CSharpCompilation.Create(
            assemblyName: "Sample",
            references: Enumerable
                .Concat(dotNetAssemblies, stdbAssemblies)
                .Select(assemblyPath => MetadataReference.CreateFromFile(assemblyPath)),
            options: new(
                OutputKind.NetModule,
                nullableContextOptions: NullableContextOptions.Enable
            ),
            syntaxTrees: [CSharpSyntaxTree.ParseText(SourceText.From(sampleSource))]
        );
    }

    record struct StepOutput(string Key, IncrementalStepRunReason Reason, object Value);

    [Theory]
    [InlineData(typeof(SpacetimeDB.Codegen.Module))]
    [InlineData(typeof(SpacetimeDB.Codegen.Type))]
    public static Task VerifyDriver(Type generatorType)
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
        var genDriver = driver.RunGenerators(SampleCompilation);
        // Run again with a new compilation to see if the cache is working.
        var regenDriver = genDriver.RunGenerators(SampleCompilation.Clone());

        var regenSteps = regenDriver
            .GetRunResult()
            .Results[0]
            .TrackedSteps.Where(step => step.Key.StartsWith("SpacetimeDB."))
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

        return Verify(genDriver).UseFileName(generatorType.Name);
    }
}
