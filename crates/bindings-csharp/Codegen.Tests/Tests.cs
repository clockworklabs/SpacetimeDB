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
    private static string GetDotNetDir() =>
        Path.GetDirectoryName(typeof(object).Assembly.Location)!;

    private static string GetProjectDirectory([CallerFilePath] string? currentFile = null) =>
        Path.GetDirectoryName(currentFile)!;

    private static readonly ImmutableArray<PortableExecutableReference> CompilationReferences;

    private static readonly CSharpCompilationOptions CompilationOptions =
        new(OutputKind.ConsoleApplication, nullableContextOptions: NullableContextOptions.Enable);

    private static readonly SyntaxTree SampleSource;

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

        var dotNetDir = GetDotNetDir();
        var projectDir = GetProjectDirectory();
        CompilationReferences = Enumerable
            .Concat(
                ImmutableArray
                    .Create("System.Private.CoreLib", "System.Runtime")
                    .Select(name => $"{dotNetDir}/{name}.dll"),
                ImmutableArray
                    .Create("BSATN.Runtime", "Runtime")
                    .Select(name => $"{projectDir}/../{name}/bin/Debug/net8.0/SpacetimeDB.{name}.dll")
            )
            .Select(assemblyPath => MetadataReference.CreateFromFile(assemblyPath))
            .ToImmutableArray();

        using var sample = File.OpenRead($"{projectDir}/Sample.cs");
        SampleSource = CSharpSyntaxTree.ParseText(SourceText.From(sample));
    }

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
        var driver = CSharpGeneratorDriver.Create(generator);
        // Store the new driver instance - it contains the results and the cache.
        var genDriver = driver.RunGenerators(compilation);

        return Verify(genDriver).UseFileName(generatorType.Name);
    }
}
