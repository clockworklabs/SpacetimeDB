namespace SpacetimeDB.Codegen.Tests;

using System.Collections.Immutable;
using System.Runtime.CompilerServices;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.MSBuild;
using Microsoft.CodeAnalysis.Text;

/// <summary>
/// Snapshot tests for the <c>SpacetimeDB.Codegen</c> library.
///
/// These run code generation for the sample projects in <c>fixtures</c>. We compare the generated code
/// to known-good examples of generated code using the Verify library: https://github.com/VerifyTests/Verify
///
/// If you need to update the generated code, you probably want to install the Verify.Terminal tool: https://github.com/VerifyTests/Verify.Terminal
/// Run <c>dotnet tool restore; dotnet verify accept</c> after changing the code generation to compare the old and new generated code and approve it.
/// You'll need to check the updated snapshots into Git with your PR; the .gitignores in this project are set up to add the right files.
/// </summary>
public static class GeneratorSnapshotTests
{
#if NET10_0_OR_GREATER
    private const string ModuleTargetFramework = "net10.0";
#else
    private const string ModuleTargetFramework = "net8.0";
#endif

    // Note that we can't use assembly path here because it will be put in some deep nested folder.
    // Instead, to get the test project directory, we can use the `CallerFilePath` attribute which will magically give us path to the current file.
    static string GetProjectDir([CallerFilePath] string path = "") => Path.GetDirectoryName(path)!;

    record struct StepOutput(string Key, IncrementalStepRunReason Reason, object Value);

    private class Fixture(string projectDir, CSharpCompilation sampleCompilation)
    {
        public CSharpCompilation SampleCompilation { get; } = sampleCompilation;
        public CSharpParseOptions ParseOptions { get; } =
            (CSharpParseOptions)sampleCompilation.SyntaxTrees.First().Options;

        public static async Task<Fixture> Compile(string name)
        {
            var targetFramework = name == "client" ? "netstandard2.1" : ModuleTargetFramework;
            var projectDir = Path.Combine(GetProjectDir(), "fixtures", name);
            using var workspace = MSBuildWorkspace.Create(
                new Dictionary<string, string> { ["TargetFramework"] = targetFramework }
            );
            var sampleProject = await workspace.OpenProjectAsync($"{projectDir}/{name}.csproj");
            var compilation = await sampleProject.GetCompilationAsync();
            return new(projectDir, (CSharpCompilation)compilation!);
        }

        public Task Verify(string fileName, object target)
        {
            if (
                (
                    fileName == nameof(Module)
                    || fileName == nameof(EnvironmentGenerator)
                    || fileName == "ExtraCompilationErrors"
                )
                && ModuleTargetFramework == "net10.0"
            )
            {
                fileName += ".net10";
            }
            return Verifier
                .Verify(target)
                .UseDirectory($"{projectDir}/snapshots")
                .UseFileName(fileName);
        }

        private static CSharpGeneratorDriver CreateDriver(
            IIncrementalGenerator generator,
            CSharpParseOptions parseOptions
        )
        {
            return CSharpGeneratorDriver.Create(
                [generator.AsSourceGenerator()],
                driverOptions: new(
                    disabledOutputs: IncrementalGeneratorOutputKind.None,
                    trackIncrementalGeneratorSteps: true
                ),
                // Make sure generated files are parsed with the same language version and feature flags.
                parseOptions: parseOptions
            );
        }

        private async Task<IEnumerable<SyntaxTree>> RunAndCheckGenerator(
            IIncrementalGenerator generator
        )
        {
            var driver = CreateDriver(generator, ParseOptions);

            // Store the new driver instance - it contains the results and the cache.
            var driverAfterGen = driver.RunGenerators(SampleCompilation);
            var genResult = driverAfterGen.GetRunResult();

            // Verify the generated code against the snapshots.
            await Verify(generator.GetType().Name, genResult);

            CheckCacheWorking(SampleCompilation, driverAfterGen);

            return genResult.GeneratedTrees;
        }

        public GeneratorDriverRunResult RunGeneratorAndGetResult(IIncrementalGenerator generator)
        {
            var driver = CreateDriver(generator, ParseOptions);
            return driver.RunGenerators(SampleCompilation).GetRunResult();
        }

        public async Task<CSharpCompilation> RunAndCheckGenerators(
            params IIncrementalGenerator[] generators
        ) =>
            SampleCompilation.AddSyntaxTrees(
                (await Task.WhenAll(generators.Select(RunAndCheckGenerator)))
                    .SelectMany(output => output)
                    .Concat(
                        generators.Any(generator => generator is SpacetimeDB.Codegen.Module)
                            ? RunGeneratorAndGetResult(new EnvironmentGenerator()).GeneratedTrees
                            : []
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
            .OrderBy(diag => diag.GetMessage() + diag.Location.ToString());
    }

    static void AssertGeneratedCodeDoesNotUseInternalBound(CSharpCompilation compilation)
    {
        var generatedText = string.Join(
            "\n\n",
            compilation.SyntaxTrees.Select(tree => tree.GetText().ToString())
        );

        Assert.DoesNotContain("global::SpacetimeDB.Internal.Bound<", generatedText);
        Assert.Contains("global::SpacetimeDB.Bound<", generatedText);
    }

    static void AssertPublicBoundIsAvailableInRuntime(Compilation compilation)
    {
        var bound = compilation.GetTypeByMetadataName("SpacetimeDB.Bound`1");
        Assert.NotNull(bound);
        Assert.Equal(Accessibility.Public, bound!.DeclaredAccessibility);
    }

    static void AssertContextOwnership(Compilation compilation)
    {
        var runtimeAssembly = compilation
            .References.Select(compilation.GetAssemblyOrModuleSymbol)
            .OfType<IAssemblySymbol>()
            .FirstOrDefault(a => a.Name == "SpacetimeDB.Runtime");

        Assert.NotNull(runtimeAssembly);

        // Use the fixture's target, not the test host: the .NET 10 suite also compiles .NET 8 examples.
        var sharedContexts = (
            (CSharpParseOptions)compilation.SyntaxTrees.First().Options
        ).PreprocessorSymbolNames.Contains("NET10_0_OR_GREATER");
        foreach (
            var name in new[]
            {
                "SpacetimeDB.Local",
                "SpacetimeDB.ReducerContext",
                "SpacetimeDB.ProcedureContext",
                "SpacetimeDB.ProcedureTxContext",
                "SpacetimeDB.HandlerContext",
                "SpacetimeDB.HandlerTxContext",
                "SpacetimeDB.ViewContext",
                "SpacetimeDB.AnonymousViewContext",
                "SpacetimeDB.QueryBuilder",
            }
        )
        {
            var runtimeType = runtimeAssembly!.GetTypeByMetadataName(name);
            var generatedType = compilation.Assembly.GetTypeByMetadataName(name);
            if (sharedContexts)
            {
                Assert.NotNull(runtimeType);
                Assert.Equal(Accessibility.Public, runtimeType!.DeclaredAccessibility);
                Assert.Null(generatedType);
            }
            else
            {
                Assert.Null(runtimeType);
                Assert.NotNull(generatedType);
            }
            Assert.True(
                SymbolEqualityComparer.Default.Equals(
                    sharedContexts ? runtimeType : generatedType,
                    compilation.GetTypeByMetadataName(name)
                ),
                $"{name} must resolve to its owning assembly without ambiguity."
            );
        }

        // The legacy runtime shell remains on .NET 8, where generated code shadows it.
        var readOnlyName = "SpacetimeDB.Internal.LocalReadOnly";
        Assert.NotNull(runtimeAssembly!.GetTypeByMetadataName(readOnlyName));
        if (sharedContexts)
        {
            Assert.Null(compilation.Assembly.GetTypeByMetadataName(readOnlyName));
        }
        else
        {
            Assert.NotNull(compilation.Assembly.GetTypeByMetadataName(readOnlyName));
        }
    }

    static void AssertNoCs0436Diagnostics(Compilation compilation)
    {
        var diagnostics = compilation
            .Emit(Stream.Null)
            .Diagnostics.Where(diag => diag.Severity != DiagnosticSeverity.Hidden);

        Assert.DoesNotContain(diagnostics, d => d.Id == "CS0436");
    }

#if NET10_0_OR_GREATER
    [Fact]
    public static async Task NamespaceDeclarationsParseAndValidate()
    {
        var fixture = await Fixture.Compile("server");
        const string usings =
            "global using System; global using System.IO; "
            + "global using System.Collections.Generic; global using System.Linq;\n";
        CSharpCompilation Create(
            string name,
            string source,
            params MetadataReference[] references
        ) =>
            CSharpCompilation.Create(
                name,
                [CSharpSyntaxTree.ParseText(usings + source, fixture.ParseOptions)],
                fixture.SampleCompilation.References.Concat(references),
                new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary)
            );
        MetadataReference Dependency(string name)
        {
            var compilation = Create(
                name,
                $$"""
                namespace {{name}} {
                    public class Marker { }
                    [SpacetimeDB.Table(Accessor = "{{name}}Row")]
                    public partial struct Row { public uint Id; }
                }
                """
            );
            var driver = CSharpGeneratorDriver.Create(
                [
                    new Type().AsSourceGenerator(),
                    new Module().AsSourceGenerator(),
                    new EnvironmentGenerator().AsSourceGenerator(),
                ],
                parseOptions: fixture.ParseOptions
            );
            driver.RunGeneratorsAndUpdateCompilation(
                compilation,
                out var output,
                out var diagnostics
            );
            Assert.Empty(diagnostics.Where(d => d.Severity == DiagnosticSeverity.Error));
            using var dll = new MemoryStream();
            var emitted = output.Emit(dll);
            Assert.True(emitted.Success, string.Join("\n", emitted.Diagnostics));
            return MetadataReference.CreateFromImage(dll.ToArray());
        }
        var auth = Dependency("Auth");
        var audit = Dependency("Audit");
        string Mount(string marker = "Auth.Marker", string accessor = "MyAuth") =>
            $"[assembly: SpacetimeDB.Namespace(typeof({marker}), Accessor = \"{accessor}\")]\n";
        GeneratorDriver Run(string source) =>
            CSharpGeneratorDriver
                .Create(
                    [new Module().AsSourceGenerator()],
                    parseOptions: fixture.ParseOptions,
                    driverOptions: new GeneratorDriverOptions(
                        IncrementalGeneratorOutputKind.None,
                        trackIncrementalGeneratorSteps: true
                    )
                )
                .RunGenerators(Create("Consumer", source, auth, audit));
        void Reject(string source, string message)
        {
            var diagnostics = Run(source).GetRunResult().Diagnostics;
            Assert.Contains(diagnostics, d => d.GetMessage().Contains(message));
            Assert.All(diagnostics, d => Assert.True(d.Location.IsInSource, d.ToString()));
        }

        object[] Parsed(GeneratorDriver driver) =>
            [
                .. driver
                    .GetRunResult()
                    .Results.Single()
                    .TrackedSteps["SpacetimeDB.Namespace.Parse"]
                    .Single()
                    .Outputs.SelectMany(o =>
                        ((System.Collections.IEnumerable)o.Value).Cast<object>()
                    ),
            ];
        Assert.Empty(Run(Mount(accessor: new string('a', 63))).GetRunResult().Diagnostics);
        Assert.Empty(Run(Mount(accessor: "public")).GetRunResult().Diagnostics);
        Reject(Mount(accessor: new string('a', 64)), "63 UTF-8 bytes");
        foreach (var name in new[] { "", "auth.data", "a-b", "1auth", " auth" })
            Reject(Mount(accessor: name), "database identifier");
        foreach (var name in new[] { "st", "ST", "spacetimedb", "pg_catalog", "PG_temp" })
            Reject(Mount(accessor: name), "reserved");
        foreach (var name in new[] { "GetType", "ToString", "Equals", "GetHashCode" })
            Reject(Mount(accessor: name), "receiver member");
        foreach (var accessor in new[] { "", "a.b", "a-b", "1auth", "@class", " auth" })
            Reject(Mount(accessor: accessor), "C# identifier");
        Reject("[assembly: SpacetimeDB.Namespace(typeof(Auth.Marker))]", "C# identifier");
        Reject("[assembly: SpacetimeDB.Namespace(null)]", "marker type");
        Reject(Mount("LocalMarker") + "public class LocalMarker { }", "cannot mount itself");
        Reject(Mount("System.String"), "no discovered module descriptor");
        Reject(Mount() + Mount(accessor: "Other"), "only be mounted once");
        Reject(Mount() + Mount("Audit.Marker", "MYAUTH"), "case-insensitive");
        Reject(Mount() + Mount("Audit.Marker", "MyAuth"), "accessor 'MyAuth'");
        Reject(
            Mount()
                + "[SpacetimeDB.Table(Accessor = \"MyAuth\")] public partial struct Row { public uint Id; }",
            "root table accessor"
        );

        var oldLanguage = fixture.ParseOptions.WithLanguageVersion(LanguageVersion.CSharp13);
        var oldCompilation = Create("Consumer", "", auth, audit)
            .RemoveAllSyntaxTrees()
            .AddSyntaxTrees(CSharpSyntaxTree.ParseText(usings + Mount(), oldLanguage));
        var oldResult = CSharpGeneratorDriver
            .Create([new Module().AsSourceGenerator()], parseOptions: oldLanguage)
            .RunGenerators(oldCompilation)
            .GetRunResult();
        Assert.Contains(
            oldResult.Diagnostics,
            d => d.GetMessage().Contains("require .NET 10 and C# 14")
        );

        var original = Run(Mount());
        foreach (var source in new[] { Mount(accessor: "Other"), Mount(accessor: "class") })
        {
            var changed = original.RunGenerators(Create("Consumer", source, auth, audit));
            Assert.Empty(changed.GetRunResult().Diagnostics);
            Assert.NotEqual(Assert.Single(Parsed(original)), Assert.Single(Parsed(changed)));
            Assert.Contains(
                changed
                    .GetRunResult()
                    .Results.Single()
                    .TrackedSteps["SpacetimeDB.Namespace.Parse"]
                    .SelectMany(s => s.Outputs),
                o => o.Reason == IncrementalStepRunReason.Modified
            );
        }
    }

    [Fact]
    public static async Task NamespaceGeneratedNameCollisions()
    {
        var fixture = await Fixture.Compile("server");
        (Compilation Output, ImmutableArray<Diagnostic> Diagnostics) Generate(string source)
        {
            var compilation = CSharpCompilation.Create(
                "CollisionProof",
                [
                    CSharpSyntaxTree.ParseText(
                        "global using System; global using System.IO; global using System.Collections.Generic;\n"
                            + source,
                        fixture.ParseOptions
                    ),
                ],
                fixture.SampleCompilation.References,
                new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary)
            );
            CSharpGeneratorDriver
                .Create(
                    [
                        new Type().AsSourceGenerator(),
                        new Module().AsSourceGenerator(),
                        new EnvironmentGenerator().AsSourceGenerator(),
                    ],
                    parseOptions: fixture.ParseOptions
                )
                .RunGeneratorsAndUpdateCompilation(
                    compilation,
                    out var output,
                    out var diagnostics
                );
            return (output, diagnostics);
        }
        foreach (
            var (accessor, fields, symbol) in new[]
            {
                ("User", "[SpacetimeDB.Unique] public uint Count;", "Count"),
                (
                    "User",
                    "[SpacetimeDB.Unique] public uint Id; [SpacetimeDB.Unique] public uint __Id;",
                    "__Id"
                ),
                (
                    "User",
                    "[SpacetimeDB.Unique] public uint Id; [SpacetimeDB.Unique] public uint IdUniqueIndex;",
                    "IdUniqueIndex"
                ),
                ("User", "public uint UserCols;", "UserCols"),
                ("User", "[SpacetimeDB.PrimaryKey] public uint UserIxCols;", "UserIxCols"),
                ("Tables", "public uint Id;", "Tables"),
                ("ReadOnlyTables", "public uint Id;", "ReadOnlyTables"),
                ("Queries", "public uint Id;", "Queries"),
                ("GetType", "public uint Id;", "GetType"),
            }
        )
        {
            var (_, diagnostics) = Generate(
                $$"""
                [SpacetimeDB.Table(Accessor = "{{accessor}}")]
                public partial struct Row { {{fields}} }
                """
            );
            Assert.DoesNotContain(diagnostics, d => d.Id == "CS8785");
            Assert.True(
                diagnostics.Any(d =>
                    d.GetMessage().Contains("Generated C# name")
                    && d.GetMessage().Contains(symbol)
                    && d.Location.IsInSource
                ),
                $"Expected collision for {accessor}.{symbol}: {string.Join("\n", diagnostics)}"
            );
        }
        var (_, crossTableDiagnostics) = Generate(
            """
            [SpacetimeDB.Table(Accessor = "User")]
            [SpacetimeDB.Table(Accessor = "UserIx")]
            public partial struct Row { public uint Id; }
            """
        );
        Assert.Contains(
            crossTableDiagnostics,
            d =>
                d.GetMessage().Contains("UserIxCols")
                && d.GetMessage().Contains("table 'User'")
                && d.GetMessage().Contains("table 'UserIx'")
        );

        var (valid, validDiagnostics) = Generate(
            """
            [SpacetimeDB.Table(Accessor = "First")]
            [SpacetimeDB.Table(Accessor = "Second")]
            public partial struct Row { [SpacetimeDB.Unique] public uint @class; }
            """
        );
        Assert.Empty(validDiagnostics.Where(d => d.Severity == DiagnosticSeverity.Error));
        using var dll = new MemoryStream();
        var emitted = valid.Emit(dll);
        Assert.True(emitted.Success, string.Join("\n", emitted.Diagnostics));
    }
#endif

    [Fact]
    public static async Task TypeGeneratorOnClient()
    {
        var fixture = await Fixture.Compile("client");

        var compilationAfterGen = await fixture.RunAndCheckGenerators(
            new SpacetimeDB.Codegen.Type()
        );

        Assert.Empty(GetCompilationErrors(compilationAfterGen));
    }

#if NET10_0_OR_GREATER
    [Theory]
    [InlineData(false, false)]
    [InlineData(true, false)]
    [InlineData(false, true)]
    [InlineData(true, true)]
    public static async Task NamespaceDependenciesRegisterOnceInStableOrder(
        bool rootHasTable,
        bool mounted
    )
    {
        var fixture = await Fixture.Compile("server");
        const string usings =
            "global using System; global using System.IO; "
            + "global using System.Collections.Generic; global using System.Linq;\n";
        CSharpCompilation Create(
            string name,
            string source,
            params MetadataReference[] references
        ) =>
            CSharpCompilation.Create(
                name,
                [CSharpSyntaxTree.ParseText(usings + source, fixture.ParseOptions)],
                fixture.SampleCompilation.References.Concat(references),
                new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary)
            );
        CSharpCompilation Generate(CSharpCompilation input)
        {
            var driver = CSharpGeneratorDriver.Create(
                [
                    new Type().AsSourceGenerator(),
                    new Module().AsSourceGenerator(),
                    new EnvironmentGenerator().AsSourceGenerator(),
                ],
                parseOptions: fixture.ParseOptions
            );
            driver.RunGeneratorsAndUpdateCompilation(input, out var output, out var diagnostics);
            Assert.Empty(diagnostics.Where(d => d.Severity == DiagnosticSeverity.Error));
            Assert.Empty(GetCompilationErrors(output));
            return (CSharpCompilation)output;
        }
        MetadataReference Emit(CSharpCompilation compilation)
        {
            using var dll = new MemoryStream();
            var result = compilation.Emit(dll);
            Assert.True(result.Success, string.Join("\n", result.Diagnostics));
            return MetadataReference.CreateFromImage(dll.ToArray());
        }
        string Table(string name) =>
            $$"""
                namespace {{name}} {
                    public class Sentinel { }
                    [SpacetimeDB.Table]
                    public partial struct {{name}}Row { public uint Id; }
                }
                """;
        string Descriptor(CSharpCompilation compilation) =>
            Assert
                .Single(compilation.GetSymbolsWithName("AssemblyDescriptor", SymbolFilter.Type))
                .ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat);
        MethodDeclarationSyntax Method(CSharpCompilation compilation, string name) =>
            Assert.Single(
                compilation
                    .SyntaxTrees.SelectMany(tree => tree.GetRoot().DescendantNodes())
                    .OfType<ClassDeclarationSyntax>()
                    .Where(type => type.Identifier.ValueText == "ModuleRegistration")
                    .SelectMany(type => type.Members.OfType<MethodDeclarationSyntax>())
                    .Where(method => method.Identifier.ValueText == name)
            );

        var sharedCompilation = Generate(Create("Shared", Table("Shared")));
        var shared = Emit(sharedCompilation);
        var alphaCompilation = Generate(
            Create(
                "Alpha",
                Table("Alpha") + "public class AlphaLink { public Shared.Sentinel Value; }",
                shared
            )
        );
        var alpha = Emit(alphaCompilation);
        var betaCompilation = Generate(
            Create(
                "Beta",
                Table("Beta") + "public class BetaLink { public Shared.Sentinel Value; }",
                shared
            )
        );
        var beta = Emit(betaCompilation);
        var utility = Emit(
            Create("Utility", "public class UtilityLink { public Shared.Sentinel Value; }", shared)
        );
        var rootSource = rootHasTable ? Table("Root") : "";
        if (mounted)
        {
            rootSource =
                "[assembly: SpacetimeDB.Namespace(typeof(Alpha.Sentinel), Accessor = \"Auth\")]\n"
                + "[assembly: SpacetimeDB.Namespace(typeof(Beta.Sentinel), Accessor = \"class\")]\n"
                + rootSource;
        }

        var root = Generate(Create("Root", rootSource, beta, utility, shared, alpha));
        var reordered = Generate(Create("Root", rootSource, alpha, shared, utility, beta));

        string[] Calls(CSharpCompilation compilation) =>
            [
                .. Method(compilation, "Initialize")
                    .DescendantNodes()
                    .OfType<InvocationExpressionSyntax>()
                    .Select(call => call.Expression.ToString())
                    .Where(call => call.EndsWith(".Register")),
            ];
        CSharpCompilation[] orderedDependencies = mounted
            ? [sharedCompilation, alphaCompilation, betaCompilation]
            : [alphaCompilation, betaCompilation, sharedCompilation];
        var expected = new[] { Descriptor(root) + ".Register" }
            .Concat(orderedDependencies.Select(c => Descriptor(c) + ".Register"))
            .ToArray();
        if (mounted)
        {
            var init = Method(root, "Initialize");
            var submodules = init.DescendantNodes()
                .OfType<InvocationExpressionSyntax>()
                .Where(call => call.Expression.ToString().EndsWith(".RegisterSubmodule"))
                .ToArray();
            Assert.Equal(
                ["\"Auth\"", "\"class\""],
                submodules.Select(call => call.ArgumentList.Arguments[0].ToString())
            );
        }
        Assert.Equal(expected, Calls(root));
        Assert.Equal(expected, Calls(reordered));

        if (rootHasTable || mounted)
        {
            return;
        }

        foreach (var accessor in new[] { "public", "PUBLIC" })
        {
            var publicConsumer = Generate(
                Create(
                    "PublicAccessorConsumer",
                    $$"""
                    [assembly: SpacetimeDB.Namespace(typeof(Alpha.Sentinel), Accessor = "{{accessor}}")]
                    public static class Helpers {
                        public static void Insert(SpacetimeDB.ReducerContext ctx) =>
                            ctx.Db.AlphaRow.Insert(new Alpha.AlphaRow { Id = 1 });
                        public static ulong Count(SpacetimeDB.ViewContext ctx) => ctx.Db.AlphaRow.Count;
                        public static ulong Count(SpacetimeDB.AnonymousViewContext ctx) => ctx.Db.AlphaRow.Count;
                        public static SpacetimeDB.IQuery<Alpha.AlphaRow> Query(SpacetimeDB.ViewContext ctx) =>
                            ctx.From.AlphaRow();
                        public static SpacetimeDB.IQuery<Alpha.AlphaRow> Query(SpacetimeDB.AnonymousViewContext ctx) =>
                            ctx.From.AlphaRow();
                    }
                    """,
                    alpha,
                    shared
                )
            );
            Assert.Equal(
                new[]
                {
                    Descriptor(publicConsumer) + ".Register",
                    Descriptor(alphaCompilation) + ".Register",
                    Descriptor(sharedCompilation) + ".Register",
                },
                Calls(publicConsumer)
            );
            Assert.DoesNotContain(
                "RegisterSubmodule",
                Method(publicConsumer, "Initialize").ToString()
            );
        }

        string HttpModule(string name) =>
            $$"""
                namespace {{name}}Module {
                    using SpacetimeDB;
                    public static partial class Functions {
                        [HttpHandler]
                        public static HttpResponse {{name}}(HandlerContext ctx, HttpRequest request) =>
                            throw new Exception();
                        [HttpRouter]
                        public static Router Routes() => Router.New()
                            .Get("/short", Handlers.{{name}})
                            .Get("/qualified", global::SpacetimeDB.Handlers.{{name}});
                    }
                }
                """;
        // The library references a table-only module, which also generates a Handlers container.
        var httpLibrary = Emit(
            Generate(Create("HttpLibrary", HttpModule("LibraryHandler"), alpha, shared))
        );
        Generate(Create("HttpConsumer", HttpModule("RootHandler"), httpLibrary, alpha, shared));

        string Policy(string? policy) =>
            policy is null
                ? ""
                : $$"""
                    public static class NamingSettings {
                        [SpacetimeDB.Settings]
                        public const SpacetimeDB.CaseConversionPolicy Naming = SpacetimeDB.CaseConversionPolicy.{{policy}};
                    }
                    """;
        foreach (
            var (rootPolicy, dependencyPolicy) in new (string?, string?)[]
            {
                ("SnakeCase", "None"),
                (null, "None"),
                ("None", "SnakeCase"),
                ("None", "None"),
                ("SnakeCase", "SnakeCase"),
                (null, "SnakeCase"),
                ("None", null),
            }
        )
        {
            var dependency = Emit(
                Generate(
                    Create("PolicyDependency", Table("PolicyDependency") + Policy(dependencyPolicy))
                )
            );
            foreach (var mount in new[] { "", "public", "Named" })
            {
                var source =
                    (
                        mount.Length == 0
                            ? ""
                            : $"[assembly: SpacetimeDB.Namespace(typeof(PolicyDependency.Sentinel), Accessor = \"{mount}\")]\n"
                    ) + Policy(rootPolicy);
                var compilation = Create("PolicyConsumer", source, dependency);
                if (
                    mount != "Named"
                    && dependencyPolicy is not null
                    && dependencyPolicy != (rootPolicy ?? "SnakeCase")
                )
                {
                    var result = CSharpGeneratorDriver
                        .Create(
                            [new Module().AsSourceGenerator()],
                            parseOptions: fixture.ParseOptions
                        )
                        .RunGenerators(compilation)
                        .GetRunResult();
                    Assert.Contains(
                        result.Diagnostics,
                        diagnostic =>
                            diagnostic.Severity == DiagnosticSeverity.Error
                            && diagnostic.Descriptor.Title.ToString()
                                == "Conflicting case conversion policies"
                            && diagnostic.GetMessage().Contains("PolicyConsumer")
                            && diagnostic.GetMessage().Contains("PolicyDependency")
                            && diagnostic.GetMessage().Contains("SnakeCase")
                            && diagnostic.GetMessage().Contains("None")
                    );
                }
                else
                {
                    Generate(compilation);
                }
            }
        }

        // An unrelated utility alone must not cause an otherwise empty module to register.
        var plainUtility = Emit(Create("PlainUtility", "public class PlainUtility { }"));
        var empty = Generate(Create("Empty", "", plainUtility));
        Assert.Empty(empty.GetSymbolsWithName("AssemblyDescriptor", SymbolFilter.Type));

        var nested = Emit(
            Generate(
                Create(
                    "Nested",
                    "[assembly: SpacetimeDB.Namespace(typeof(Alpha.Sentinel), Accessor = \"Auth\")]",
                    alpha,
                    shared
                )
            )
        );
        var nestedResult = CSharpGeneratorDriver
            .Create([new Module().AsSourceGenerator()], parseOptions: fixture.ParseOptions)
            .RunGenerators(Create("Outer", "", nested, alpha, shared))
            .GetRunResult();
        Assert.Contains(
            nestedResult.Diagnostics,
            diagnostic => diagnostic.GetMessage().Contains("Only the consuming root")
        );

        foreach (var kind in new[] { "Init", "ClientConnected", "ClientDisconnected" })
        {
            var lifecycle = Emit(
                Generate(
                    Create(
                        "LifecycleDependency",
                        $$"""
                        public class Marker { }
                        public static partial class LifecycleFunctions {
                            [SpacetimeDB.Reducer(SpacetimeDB.ReducerKind.{{kind}})]
                            public static void Handle(SpacetimeDB.ReducerContext ctx) { }
                        }
                        """
                    )
                )
            );
            var lifecycleResult = CSharpGeneratorDriver
                .Create([new Module().AsSourceGenerator()], parseOptions: fixture.ParseOptions)
                .RunGenerators(
                    Create(
                        "LifecycleConsumer",
                        "[assembly: SpacetimeDB.Namespace(typeof(Marker), Accessor = \"Auth\")]",
                        lifecycle
                    )
                )
                .GetRunResult();
            Assert.Contains(
                lifecycleResult.Diagnostics,
                diagnostic =>
                    diagnostic.Severity == DiagnosticSeverity.Error
                    && diagnostic.Descriptor.Title.ToString()
                        == "Root-only declarations in mounted dependency"
                    && diagnostic.GetMessage().Contains("LifecycleFunctions.Handle (" + kind + ")")
                    && diagnostic.GetMessage().Contains("Auth")
            );
            // The same dependency can still be published alone or merged into the root scope.
            Generate(Create("FlatLifecycleConsumer", "", lifecycle));
            Generate(
                Create(
                    "PublicLifecycleConsumer",
                    "[assembly: SpacetimeDB.Namespace(typeof(Marker), Accessor = \"public\")]",
                    lifecycle
                )
            );
        }

        foreach (
            var (source, declaration) in new[]
            {
                (
                    "#pragma warning disable STDB_UNSTABLE\n"
                        + """
                        public static class Rules {
                            [SpacetimeDB.ClientVisibilityFilter]
                            public static readonly SpacetimeDB.Filter Visible =
                                new SpacetimeDB.Filter.Sql("SELECT * FROM Entry");
                        }
                        """,
                    "row-level security filters"
                ),
                (
                    "[SpacetimeDB.Env] public struct Settings { public string SECRET; }",
                    "environment variables"
                ),
            }
        )
        {
            // These declarations must register even without tables or functions in the assembly.
            var dependencyCompilation = Generate(
                Create("RestrictedDependency", source + "\npublic class Entry { }")
            );
            var dependencyDescriptor = Descriptor(dependencyCompilation);
            var dependency = Emit(dependencyCompilation);
            if (declaration == "row-level security filters")
            {
                Assert.Contains(
                    Method(dependencyCompilation, "Register")
                        .DescendantNodes()
                        .OfType<InvocationExpressionSyntax>(),
                    call =>
                        call.Expression.ToString() == "builder.RegisterClientVisibilityFilter"
                        && call.ArgumentList.Arguments.Single().ToString()
                            == "global::Rules.Visible"
                );
            }
            var result = CSharpGeneratorDriver
                .Create([new Module().AsSourceGenerator()], parseOptions: fixture.ParseOptions)
                .RunGenerators(
                    Create(
                        "RestrictedConsumer",
                        "[assembly: SpacetimeDB.Namespace(typeof(Entry), Accessor = \"Auth\")]",
                        dependency
                    )
                )
                .GetRunResult();
            Assert.Contains(
                result.Diagnostics,
                diagnostic =>
                    diagnostic.Severity == DiagnosticSeverity.Error
                    && diagnostic.GetMessage().Contains("RestrictedDependency")
                    && diagnostic.GetMessage().Contains("'Auth'")
                    && diagnostic.GetMessage().Contains(declaration)
                    && diagnostic.GetMessage().Contains("root scope")
            );
            foreach (
                var mount in new[]
                {
                    "",
                    "[assembly: SpacetimeDB.Namespace(typeof(Entry), Accessor = \"public\")]",
                }
            )
            {
                var consumer = Generate(Create("PublicConsumer", mount, dependency));
                Assert.Equal(
                    new[]
                    {
                        Descriptor(consumer) + ".Register",
                        dependencyDescriptor + ".Register",
                    },
                    Calls(consumer)
                );
            }
        }

        // Empty environment schemas contain no keys and are permitted by the host.
        var emptyEnvironment = Emit(
            Generate(Create("EmptyEnvironment", "[SpacetimeDB.Env] public struct Settings { }"))
        );
        Generate(
            Create(
                "EmptyEnvironmentConsumer",
                "[assembly: SpacetimeDB.Namespace(typeof(Settings), Accessor = \"Auth\")]",
                emptyEnvironment
            )
        );
    }
#endif

    [Fact]
    public static async Task TypeAndModuleGeneratorsOnServer()
    {
        var fixture = await Fixture.Compile("server");
        await fixture.Verify(
            nameof(EnvironmentGenerator),
            fixture.RunGeneratorAndGetResult(new EnvironmentGenerator())
        );

        var compilationAfterGen = await fixture.RunAndCheckGenerators(
            new SpacetimeDB.Codegen.Type(),
            new SpacetimeDB.Codegen.Module()
        );

        Assert.Empty(GetCompilationErrors(compilationAfterGen));

        AssertPublicBoundIsAvailableInRuntime(compilationAfterGen);
        AssertContextOwnership(compilationAfterGen);
        AssertGeneratedCodeDoesNotUseInternalBound(compilationAfterGen);

        // Regression guard for user-reported warning spam:
        // make sure a downstream "user" file that references SpacetimeDB.Local doesn't trigger CS0436.
        var userCode =
            "namespace User; public sealed class UseLocal { public SpacetimeDB.Local Db; }";
        var userTree = CSharpSyntaxTree.ParseText(userCode, fixture.ParseOptions);
        var compilationWithUserCode = compilationAfterGen.AddSyntaxTrees(userTree);
        AssertNoCs0436Diagnostics(compilationWithUserCode);
    }

    [Fact]
    public static async Task SettingsAndExplicitNames()
    {
        var fixture = await Fixture.Compile("explicitnames");

        var compilationAfterGen = await fixture.RunAndCheckGenerators(
            new SpacetimeDB.Codegen.Type(),
            new SpacetimeDB.Codegen.Module()
        );

        Assert.Empty(GetCompilationErrors(compilationAfterGen));

        AssertPublicBoundIsAvailableInRuntime(compilationAfterGen);
        AssertContextOwnership(compilationAfterGen);
        AssertGeneratedCodeDoesNotUseInternalBound(compilationAfterGen);
    }

    [Fact]
    public static async Task CSharpKeywordIdentifiersAreEscapedInGeneratedCode()
    {
        var fixture = await Fixture.Compile("server");

        const string source = """
            using SpacetimeDB;

            [SpacetimeDB.Table]
            public partial struct KeywordTable
            {
                [SpacetimeDB.PrimaryKey]
                public ulong @class;

                public int @params;
            }

            [SpacetimeDB.Table(Accessor = "event")]
            public partial struct AccessorKeywordTable
            {
                [SpacetimeDB.PrimaryKey]
                [SpacetimeDB.Index.BTree(Accessor = "params")]
                public int Id;
            }

            [SpacetimeDB.Table]
            public partial struct @class
            {
                [SpacetimeDB.PrimaryKey]
                public int Id;
            }

            [SpacetimeDB.Table]
            public partial struct TimestampPrimaryKeyTable
            {
                [SpacetimeDB.PrimaryKey]
                public Timestamp CreatedAt;
            }

            public static partial class KeywordApis
            {
                [SpacetimeDB.Reducer]
                public static void KeywordReducer(ReducerContext ctx, int @params, string @class)
                {
                    _ = @params;
                    _ = @class;
                }

                [SpacetimeDB.Reducer]
                public static void @class(ReducerContext ctx)
                {
                }

                [SpacetimeDB.Procedure]
                public static int KeywordProcedure(ProcedureContext ctx, int @params, int @class)
                {
                    return @params + @class;
                }

                [SpacetimeDB.Procedure]
                public static void @params(ProcedureContext ctx)
                {
                }
            }
            """;

        var tree = CSharpSyntaxTree.ParseText(
            source,
            fixture.ParseOptions,
            path: "KeywordNames.cs"
        );
        var compilation = fixture.SampleCompilation.AddSyntaxTrees(tree);

        var driver = CSharpGeneratorDriver.Create(
            [
                new SpacetimeDB.Codegen.Type().AsSourceGenerator(),
                new SpacetimeDB.Codegen.Module().AsSourceGenerator(),
                new EnvironmentGenerator().AsSourceGenerator(),
            ],
            driverOptions: new(
                disabledOutputs: IncrementalGeneratorOutputKind.None,
                trackIncrementalGeneratorSteps: true
            ),
            parseOptions: fixture.ParseOptions
        );

        var runResult = driver.RunGenerators(compilation).GetRunResult();
        var compilationAfterGen = compilation.AddSyntaxTrees(runResult.GeneratedTrees);

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

        AssertPublicBoundIsAvailableInRuntime(compilationAfterGen);
        AssertContextOwnership(compilationAfterGen);
        AssertGeneratedCodeDoesNotUseInternalBound(compilationAfterGen);
    }

    [Fact]
    public static async Task ViewInvalidReturnHighlightsReturnType()
    {
        var fixture = await Fixture.Compile("diag");

        var runResult = fixture.RunGeneratorAndGetResult(new SpacetimeDB.Codegen.Module());

        var method = fixture
            .SampleCompilation.SyntaxTrees.Select(tree => new
            {
                Tree = tree,
                Root = tree.GetRoot(),
            })
            .SelectMany(entry =>
                entry
                    .Root.DescendantNodes()
                    .OfType<MethodDeclarationSyntax>()
                    .Select(method => new
                    {
                        entry.Tree,
                        entry.Root,
                        Method = method,
                    })
            )
            .Single(entry => entry.Method.Identifier.Text == "ViewDefWrongReturn");

        var returnTypeSpan = method.Method.ReturnType.Span;
        var diagnostics = runResult
            .Results.SelectMany(result => result.Diagnostics)
            .Where(d => d.Id == "STDB0024")
            .ToList();
        var diagnostic = diagnostics.FirstOrDefault(d =>
            d.GetMessage().Contains("ViewDefWrongReturn") && d.Location.SourceTree == method.Tree
        );

        Assert.NotNull(diagnostic);

        Assert.Equal(returnTypeSpan, diagnostic!.Location.SourceSpan);

        var returnTypeText = method
            .Root.ToFullString()
            .Substring(returnTypeSpan.Start, returnTypeSpan.Length);
        Assert.Contains("Player", returnTypeText);
    }
}
