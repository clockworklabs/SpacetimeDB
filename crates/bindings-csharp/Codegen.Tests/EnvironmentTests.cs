namespace SpacetimeDB.Codegen.Tests;

using System.Reflection;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

public static class EnvironmentTests
{
    // A controlled host seam allows the generated accessor code to execute.
    // Live value access remains delegated to DatabaseEnvironment.Get.
    private const string Host = """
        #nullable enable
        namespace SpacetimeDB {
            [System.AttributeUsage(System.AttributeTargets.Struct)]
            public sealed class EnvAttribute : System.Attribute {}
            [System.AttributeUsage(System.AttributeTargets.Field)]
            public sealed class EnvValuesAttribute(params string[] values) : System.Attribute {}
            public readonly struct DatabaseEnvironment {
                public string? Get(string key) => Host.Get(key);
            }
            public static class Host {
                public static int Reads;
                public static string? Get(string key) {
                    Reads++;
                    return key switch {
                        "REQUIRED" => Reads.ToString(), "OPTIONAL" => null,
                        "MODE" => "prod", "Get" => "reserved", "class" => "keyword",
                        _ => throw new System.InvalidOperationException("undeclared environment key")
                    };
                }
            }
        }
        namespace SpacetimeDB.Internal {
            public abstract record EnvironmentConstraint {
                public sealed record AnyString(System.ValueTuple Value) : EnvironmentConstraint;
                public sealed record Literal(string Value) : EnvironmentConstraint;
                public sealed record OneOf(System.Collections.Generic.List<string> Value) : EnvironmentConstraint;
            }
            public sealed record EnvironmentDeclaration(string Name, EnvironmentConstraint Constraint, bool Optional);
            public static class Module {
                public static System.Collections.Generic.List<EnvironmentDeclaration> Declarations = new();
                public static void RegisterEnvironment(EnvironmentDeclaration value) => Declarations.Add(value);
            }
        }
        """;

    private static (Compilation Compilation, GeneratorDriverRunResult Result) Generate(
        string declaration
    )
    {
        var references = ((string)AppContext.GetData("TRUSTED_PLATFORM_ASSEMBLIES")!)
            .Split(Path.PathSeparator)
            .Select(path => MetadataReference.CreateFromFile(path));
        var parse = new CSharpParseOptions(LanguageVersion.Preview);
        var compilation = CSharpCompilation.Create(
            "EnvironmentFixture" + Guid.NewGuid().ToString("N"),
            [CSharpSyntaxTree.ParseText(Host + declaration, parse)],
            references,
            new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary)
        );
        GeneratorDriver driver = CSharpGeneratorDriver.Create(
            [new EnvironmentGenerator().AsSourceGenerator()],
            parseOptions: parse
        );
        driver = driver.RunGeneratorsAndUpdateCompilation(compilation, out var output, out _);
        return (output, driver.GetRunResult());
    }

    [Fact]
    public static void NamedAccessorsKeepCheckedReadsAndRegisterCanonicalConstraints()
    {
        var (compilation, result) = Generate(
            """
            [SpacetimeDB.Env] public struct Declarations {
                public string REQUIRED;
                public string? OPTIONAL;
                [SpacetimeDB.EnvValues("prod", "dev")] public string MODE;
                [SpacetimeDB.EnvValues("reserved")] public string Get;
                public string @class;
            }
            public static class Usage {
                public static void Check() {
                    var env = new SpacetimeDB.ModuleEnvironment();
                    if (env.REQUIRED == env.REQUIRED || env.OPTIONAL != null || env.MODE != "prod" ||
                        env.Get("Get") != "reserved" || env.@class != "keyword") throw new System.Exception("bad accessor");
                    try { env.Get("UNKNOWN"); throw new System.Exception("unchecked generic read"); }
                    catch (System.InvalidOperationException) {}
                    var declarations = SpacetimeDB.Internal.Module.Declarations;
                    if (declarations.Count != 5 || declarations[0].Optional || !declarations[1].Optional ||
                        declarations[0].Constraint is not SpacetimeDB.Internal.EnvironmentConstraint.AnyString ||
                        declarations[2].Constraint is not SpacetimeDB.Internal.EnvironmentConstraint.OneOf { Value.Count: 2 } ||
                        declarations[3].Constraint is not SpacetimeDB.Internal.EnvironmentConstraint.Literal { Value: "reserved" })
                        throw new System.Exception("bad metadata");
                }
            }
            """
        );
        Assert.Empty(result.Diagnostics);
        using var stream = new MemoryStream();
        var emitted = compilation.Emit(stream);
        Assert.True(emitted.Success, string.Join("\n", emitted.Diagnostics));
        var assembly = Assembly.Load(stream.ToArray());
        assembly.GetType("Usage")!.GetMethod("Check")!.Invoke(null, null);
        Assert.Null(assembly.GetType("SpacetimeDB.ModuleEnvironment")!.GetProperty("Get"));
    }

    [Theory]
    [InlineData("public int BAD;")]
    [InlineData("public static string BAD;")]
    [InlineData("[SpacetimeDB.EnvValues()] public string BAD;")]
    [InlineData("[SpacetimeDB.EnvValues(\"x\", \"x\")] public string BAD;")]
    [InlineData("[SpacetimeDB.EnvValues(null)] public string BAD;")]
    public static void InvalidDeclarationsAreCompileErrors(string field)
    {
        var (_, result) = Generate("[SpacetimeDB.Env] public struct Declarations {" + field + "}");
        Assert.Contains(
            result.Diagnostics,
            diagnostic =>
                diagnostic.Id == "STDBENV001" && diagnostic.Severity == DiagnosticSeverity.Error
        );
    }

    [Fact]
    public static void EmptySchemaRetainsOnlyGenericAccess()
    {
        var (compilation, result) = Generate("");
        Assert.Empty(result.Diagnostics);
        Assert.DoesNotContain(
            compilation.GetDiagnostics(),
            diagnostic => diagnostic.Severity == DiagnosticSeverity.Error
        );
        Assert.Contains(
            "public string? Get(string key)",
            result.GeneratedTrees.Single().ToString()
        );
    }
}
