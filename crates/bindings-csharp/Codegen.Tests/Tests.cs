namespace Codegen.Tests;

using System.Collections.Immutable;
using System.Linq;
using System.Reflection;
using System.Threading.Tasks;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Text;
using VerifyTests;
using Xunit;

public static class GeneratorSnapshotTests
{
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
    }

    private static readonly string DotNetDir = Path.GetDirectoryName(
        typeof(object).Assembly.Location
    )!;

    private static readonly ImmutableArray<PortableExecutableReference> CompilationReferences =
        Enumerable
            .Concat(
                ImmutableArray
                    .Create("System.Private.CoreLib", "System.Runtime")
                    .Select(assemblyName => Path.Join(DotNetDir, $"{assemblyName}.dll")),
                ImmutableArray
                    .Create(
                        // For `SpacetimeDB.BSATN.Runtime`.
                        typeof(SpacetimeDB.TypeAttribute),
                        // For `SpacetimeDB.Runtime`.
                        typeof(SpacetimeDB.TableAttribute)
                    )
                    .Select(type => type.Assembly.Location)
            )
            .Select(assemblyPath => MetadataReference.CreateFromFile(assemblyPath))
            .ToImmutableArray();

    private static readonly CSharpCompilationOptions CompilationOptions =
        new(OutputKind.ConsoleApplication, nullableContextOptions: NullableContextOptions.Enable);

    private static readonly SyntaxTree SampleSource = CSharpSyntaxTree.ParseText(
        SourceText.From(Assembly.GetExecutingAssembly().GetManifestResourceStream("Sample.cs")!)
    );

    record struct StepOutput(string Key, IncrementalStepRunReason Reason, object Value);

    [Theory]
    [InlineData(typeof(SpacetimeDB.Codegen.Module))]
    [InlineData(typeof(SpacetimeDB.Codegen.Type))]
    public static Task VerifyDriver(Type generatorType)
    {
        var compilation = CSharpCompilation.Create(
            assemblyName: generatorType.Name,
            references: CompilationReferences,
            options: CompilationOptions,
            syntaxTrees: [SampleSource]
        );

        var generator = (IIncrementalGenerator)Activator.CreateInstance(generatorType)!;
        var driver = CSharpGeneratorDriver.Create(
            [generator.AsSourceGenerator()],
            driverOptions: new(
                disabledOutputs: IncrementalGeneratorOutputKind.None,
                trackIncrementalGeneratorSteps: true
            )
        );
        // Store the new driver instance - it contains the cache.
        var cachedDriver = driver.RunGenerators(compilation);
        var genResult = cachedDriver.GetRunResult();

        return Verify(genResult).UseFileName(generatorType.Name);
    }
}
