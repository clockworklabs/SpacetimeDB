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

        public static async Task<Fixture> Compile(string name, string? targetFramework = null)
        {
            targetFramework ??= name == "client" ? "netstandard2.1" : ModuleTargetFramework;
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
            if ((fileName == nameof(Module) || fileName == nameof(EnvironmentGenerator) || fileName == "ExtraCompilationErrors")
                && ModuleTargetFramework == "net10.0")
            {
                fileName += ".net10";
            }
            return Verifier.Verify(target).UseDirectory($"{projectDir}/snapshots").UseFileName(fileName);
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
        var sharedContexts = ((CSharpParseOptions)compilation.SyntaxTrees.First().Options)
            .PreprocessorSymbolNames.Contains("NET10_0_OR_GREATER");
        foreach (var name in new[]
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
        })
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

    [Fact]
    // Make sure our existing C# module examples still compile when targeting .NET 8 (namespaces will only be supported on .NET 10)
    public static async Task NamespaceProofPreservesNet8RootExamples()
    {
        foreach (var name in new[] { "server", "explicitnames" })
        {
            var fixture = await Fixture.Compile(name, "net8.0");
            var compilation = fixture.SampleCompilation;
            foreach (
                var generator in new IIncrementalGenerator[]
                {
                    new SpacetimeDB.Codegen.Type(),
                    new SpacetimeDB.Codegen.Module(),
                    new EnvironmentGenerator(),
                }
            )
            {
                compilation = compilation.AddSyntaxTrees(
                    fixture.RunGeneratorAndGetResult(generator).GeneratedTrees
                );
            }
            Assert.Empty(GetCompilationErrors(compilation));
            AssertContextOwnership(compilation);
        }
    }

    [Theory]
    [InlineData("server")]
    [InlineData("explicitnames")]
    public static async Task FormattedGeneratedCodeCompiles(string fixtureName)
    {
        var fixture = await Fixture.Compile(fixtureName);
        var compilation = fixture.SampleCompilation;
        foreach (var generator in new IIncrementalGenerator[] { new Type(), new Module(), new EnvironmentGenerator() })
        {
            foreach (var tree in fixture.RunGeneratorAndGetResult(generator).GeneratedTrees)
            {
                var formatted = TestInit.FormatCode(tree.ToString());
                Assert.Empty(formatted.Errors);
                compilation = compilation.AddSyntaxTrees(
                    CSharpSyntaxTree.ParseText(
                        formatted.Code,
                        fixture.ParseOptions,
                        path: tree.FilePath
                    )
                );
            }
        }
        Assert.Empty(GetCompilationErrors(compilation));
        AssertContextOwnership(compilation);
    }

    [Theory]
    [InlineData("net8.0")]
#if NET10_0_OR_GREATER
    [InlineData("net10.0")]
#endif
    public static async Task NamespaceImmediateSchedulesUseExplicitFunctionNames(string framework)
    {
        var fixture = await Fixture.Compile("server", framework);
        var source = """
            global using System;
            global using System.IO;
            global using System.Collections.Generic;
            global using System.Linq;
            public static partial class Jobs
            {
                [SpacetimeDB.Reducer(Name = "reducer_job")]
                public static void ReducerTick(SpacetimeDB.ReducerContext ctx, uint payload) { }
                [SpacetimeDB.Procedure(Name = "procedure_job")]
                public static void ProcedureTick(SpacetimeDB.ProcedureContext ctx, uint payload) { }
                [SpacetimeDB.Reducer]
                public static void DefaultReducer(SpacetimeDB.ReducerContext ctx) { }
                [SpacetimeDB.Procedure]
                public static void DefaultProcedure(SpacetimeDB.ProcedureContext ctx) { }
            }
            """;
        var input = CSharpCompilation.Create(
            "ScheduledLibrary",
            [CSharpSyntaxTree.ParseText(source, fixture.ParseOptions)],
            fixture.SampleCompilation.References,
            new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary)
        );
        var driver = CSharpGeneratorDriver.Create(
            [new Type().AsSourceGenerator(), new Module().AsSourceGenerator(), new EnvironmentGenerator().AsSourceGenerator()],
            parseOptions: fixture.ParseOptions
        );
        driver.RunGeneratorsAndUpdateCompilation(input, out var output, out var diagnostics);
        Assert.Empty(diagnostics.Where(d => d.Severity == DiagnosticSeverity.Error));
        Assert.Empty(GetCompilationErrors(output));

        foreach (var (method, wireName) in new[]
        {
            ("ReducerTick", "reducer_job"),
            ("ProcedureTick", "procedure_job"),
            ("DefaultReducer", "DefaultReducer"),
            ("DefaultProcedure", "DefaultProcedure"),
        })
        {
            var helper = Assert.Single(output.SyntaxTrees
                .SelectMany(tree => tree.GetRoot().DescendantNodes())
                .OfType<MethodDeclarationSyntax>()
                .Where(node => node.Identifier.ValueText == "VolatileNonatomicScheduleImmediate" + method));
            var call = Assert.Single(helper.DescendantNodes()
                .OfType<InvocationExpressionSyntax>()
                .Where(node => node.Expression.ToString().EndsWith(".VolatileNonatomicScheduleImmediate")));
            var name = call.ArgumentList.Arguments[0].Expression;
            if (framework == "net10.0")
            {
                var field = Assert.IsAssignableFrom<IFieldSymbol>(output.GetSemanticModel(name.SyntaxTree)
                    .GetSymbolInfo(name).Symbol);
                Assert.True(field.IsStatic);
                Assert.True(field.IsReadOnly);
                Assert.Contains(field.ContainingType.StaticConstructors, ctor => !ctor.IsImplicitlyDeclared);
                Assert.DoesNotContain(helper.DescendantNodes().OfType<InvocationExpressionSyntax>(),
                    invocation => invocation.Expression.ToString().EndsWith(".ResolveName"));
                var declaration = Assert.IsType<VariableDeclaratorSyntax>(
                    Assert.Single(field.DeclaringSyntaxReferences).GetSyntax());
                var resolve = Assert.IsType<InvocationExpressionSyntax>(declaration.Initializer!.Value);
                Assert.Equal("global::SpacetimeDB.Internal.Module.ResolveName", resolve.Expression.ToString());
                Assert.StartsWith("ScheduledLibrary,", (string)output.GetSemanticModel(resolve.SyntaxTree)
                    .GetConstantValue(resolve.ArgumentList.Arguments[0].Expression).Value!);
                name = resolve.ArgumentList.Arguments[1].Expression;
            }
            Assert.Equal(wireName, output.GetSemanticModel(name.SyntaxTree).GetConstantValue(name).Value);
        }
    }

    [Fact]
    public static async Task NamespaceDeclarationsParseAndValidate()
    {
        var fixture = await Fixture.Compile("server");
        const string usings = "global using System; global using System.IO; "
            + "global using System.Collections.Generic; global using System.Linq;\n";
        CSharpCompilation Create(string name, string source, params MetadataReference[] references) =>
            CSharpCompilation.Create(
                name,
                [CSharpSyntaxTree.ParseText(usings + source, fixture.ParseOptions)],
                fixture.SampleCompilation.References.Concat(references),
                new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary)
            );
        MetadataReference Dependency(string name)
        {
            var compilation = Create(name, $$"""
                namespace {{name}} {
                    public class Marker { }
                    [SpacetimeDB.Table(Accessor = "{{name}}Row")]
                    public partial struct Row { public uint Id; }
                }
                """);
            var driver = CSharpGeneratorDriver.Create(
                [new Type().AsSourceGenerator(), new Module().AsSourceGenerator(), new EnvironmentGenerator().AsSourceGenerator()],
                parseOptions: fixture.ParseOptions
            );
            driver.RunGeneratorsAndUpdateCompilation(compilation, out var output, out var diagnostics);
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
        GeneratorDriver Run(string source) => CSharpGeneratorDriver.Create(
                [new Module().AsSourceGenerator()],
                parseOptions: fixture.ParseOptions,
                driverOptions: new GeneratorDriverOptions(
                    IncrementalGeneratorOutputKind.None,
                    trackIncrementalGeneratorSteps: true
                )
            ).RunGenerators(Create("Consumer", source, auth, audit));
#if NET10_0_OR_GREATER
        void Reject(string source, string message)
        {
            var diagnostics = Run(source).GetRunResult().Diagnostics;
            Assert.Contains(diagnostics, d => d.GetMessage().Contains(message));
            Assert.All(diagnostics, d => Assert.True(d.Location.IsInSource, d.ToString()));
        }

        object[] Parsed(GeneratorDriver driver) => [.. driver.GetRunResult().Results.Single()
            .TrackedSteps["SpacetimeDB.Namespace.Parse"].Single().Outputs
            .SelectMany(o => ((System.Collections.IEnumerable)o.Value).Cast<object>())];
        foreach (var accessor in new[] { "MyAuth", "class", "event" })
        {
            var driver = Run(Mount(accessor: accessor));
            Assert.Empty(driver.GetRunResult().Diagnostics);
            var declaration = Assert.Single(Parsed(driver));
            Assert.Equal(accessor, declaration.GetType().GetProperty("Accessor")!.GetValue(declaration));
            Assert.Equal(
                accessor == "MyAuth" ? accessor : "@" + accessor,
                declaration.GetType().GetProperty("AccessorIdentifier")!.GetValue(declaration)
            );
            Assert.Equal(
                "Auth, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null",
                declaration.GetType().GetProperty("AssemblyIdentity")!.GetValue(declaration)
            );
        }
        Assert.Empty(Run(Mount(accessor: new string('a', 63))).GetRunResult().Diagnostics);
        Assert.Empty(Run(Mount(accessor: "public")).GetRunResult().Diagnostics);
        Reject(Mount(accessor: new string('a', 64)), "63 UTF-8 bytes");
        foreach (var name in new[] { "", "auth.data", "a-b", "1auth", " auth" })
            Reject(Mount(accessor: name), "database identifier");
        foreach (var name in new[] { "st", "ST", "spacetimedb", "pg_catalog", "PG_temp" })
            Reject(Mount(accessor: name), "reserved");
        foreach (var accessor in new[] { "", "a.b", "a-b", "1auth", "@class", " auth" })
            Reject(Mount(accessor: accessor), "C# identifier");
        Reject("[assembly: SpacetimeDB.Namespace(typeof(Auth.Marker))]", "C# identifier");
        Reject("[assembly: SpacetimeDB.Namespace(null)]", "marker type");
        Reject(Mount("LocalMarker") + "public class LocalMarker { }", "cannot mount itself");
        Reject(Mount("System.String"), "no discovered module descriptor");
        Reject(Mount() + Mount(accessor: "Other"), "only be mounted once");
        Reject(Mount() + Mount("Audit.Marker", "MYAUTH"), "case-insensitive");
        Reject(Mount() + Mount("Audit.Marker", "MyAuth"), "accessor 'MyAuth'");
        Reject(Mount() + "[SpacetimeDB.Table(Accessor = \"MyAuth\")] public partial struct Row { public uint Id; }",
            "root table accessor");

        var oldLanguage = fixture.ParseOptions.WithLanguageVersion(LanguageVersion.CSharp13);
        var oldCompilation = Create("Consumer", "", auth, audit).RemoveAllSyntaxTrees()
            .AddSyntaxTrees(CSharpSyntaxTree.ParseText(usings + Mount(), oldLanguage));
        var oldResult = CSharpGeneratorDriver.Create(
            [new Module().AsSourceGenerator()], parseOptions: oldLanguage
        ).RunGenerators(oldCompilation).GetRunResult();
        Assert.Contains(oldResult.Diagnostics, d => d.GetMessage().Contains("require .NET 10 and C# 14"));

        // Only assembly targets are legal, independently of generator validation.
        var wrongTarget = Create("Consumer", "[SpacetimeDB.Namespace(typeof(Auth.Marker))] public class Wrong { }", auth);
        Assert.Contains(wrongTarget.GetDiagnostics(), d => d.Id == "CS0592");
        var obsoleteName = Create("Consumer",
            "[assembly: SpacetimeDB.Namespace(typeof(Auth.Marker), Accessor = \"MyAuth\", Name = \"auth_data\")]", auth);
        Assert.Contains(obsoleteName.GetDiagnostics(), d => d.Id == "CS0246");

        var original = Run(Mount());
        foreach (var source in new[] { Mount(accessor: "Other"), Mount(accessor: "class") })
        {
            var changed = original.RunGenerators(Create("Consumer", source, auth, audit));
            Assert.Empty(changed.GetRunResult().Diagnostics);
            Assert.NotEqual(Assert.Single(Parsed(original)), Assert.Single(Parsed(changed)));
            Assert.Contains(
                changed.GetRunResult().Results.Single().TrackedSteps["SpacetimeDB.Namespace.Parse"]
                    .SelectMany(s => s.Outputs),
                o => o.Reason == IncrementalStepRunReason.Modified
            );
        }
#else
        var compilation = Create("Consumer", Mount(), auth, audit);
        Assert.Null(compilation.GetTypeByMetadataName("SpacetimeDB.NamespaceAttribute"));
        Assert.Contains(compilation.GetDiagnostics(), d => d.Id == "CS0234");
        Assert.Empty(Run(Mount()).GetRunResult().Diagnostics);
#endif
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

#if NET10_0_OR_GREATER
    [Theory]
    [InlineData(false, false)]
    [InlineData(true, false)]
    [InlineData(false, true)]
    [InlineData(true, true)]
    public static async Task NamespaceDependenciesRegisterOnceInStableOrder(bool rootHasTable, bool mounted)
    {
        var fixture = await Fixture.Compile("server", "net10.0");
        const string usings = "global using System; global using System.IO; "
            + "global using System.Collections.Generic; global using System.Linq;\n";
        CSharpCompilation Create(string name, string source, params MetadataReference[] references) =>
            CSharpCompilation.Create(
                name,
                [CSharpSyntaxTree.ParseText(usings + source, fixture.ParseOptions)],
                fixture.SampleCompilation.References.Concat(references),
                new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary)
            );
        CSharpCompilation Generate(CSharpCompilation input)
        {
            var driver = CSharpGeneratorDriver.Create(
                [new Type().AsSourceGenerator(), new Module().AsSourceGenerator(), new EnvironmentGenerator().AsSourceGenerator()],
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
        string Table(string name) => $$"""
            namespace {{name}} {
                public class Sentinel { }
                [SpacetimeDB.Table]
                public partial struct {{name}}Row { public uint Id; }
                public static class QueryHelpers {
                    public static SpacetimeDB.IQuery<{{name}}Row> Query(SpacetimeDB.AnonymousViewContext ctx) => ctx.From.{{name}}Row().Where(c => c.Id.Eq(SpacetimeDB.SqlLit.Int(1u)));
                }
            }
            """;
        string Descriptor(CSharpCompilation compilation) => Assert.Single(
            compilation.GetSymbolsWithName("AssemblyDescriptor", SymbolFilter.Type)
        ).ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat);
        MethodDeclarationSyntax Method(CSharpCompilation compilation, string name) => Assert.Single(
            compilation.SyntaxTrees.SelectMany(tree => tree.GetRoot().DescendantNodes())
                .OfType<ClassDeclarationSyntax>()
                .Where(type => type.Identifier.ValueText == "ModuleRegistration")
                .SelectMany(type => type.Members.OfType<MethodDeclarationSyntax>())
                .Where(method => method.Identifier.ValueText == name)
        );

        var sharedCompilation = Generate(Create("Shared", Table("Shared")));
        var shared = Emit(sharedCompilation);
        var alphaCompilation = Generate(Create("Alpha", Table("Alpha")
            + "public class AlphaLink { public Shared.Sentinel Value; }", shared));
        var alpha = Emit(alphaCompilation);
        var betaCompilation = Generate(Create("Beta", Table("Beta")
            + "public class BetaLink { public Shared.Sentinel Value; }", shared));
        var beta = Emit(betaCompilation);
        var utility = Emit(Create("Utility",
            "public class UtilityLink { public Shared.Sentinel Value; }", shared));
        var rootSource = rootHasTable ? Table("Root") : "";
        if (mounted)
            rootSource = "[assembly: SpacetimeDB.Namespace(typeof(Alpha.Sentinel), Accessor = \"Auth\")]\n"
                + "[assembly: SpacetimeDB.Namespace(typeof(Beta.Sentinel), Accessor = \"class\")]\n"
                + rootSource
                + "public static class Helpers { public static ulong Count(SpacetimeDB.ReducerContext ctx) => ctx.Db.Auth.AlphaRow.Count + ctx.Db.@class.BetaRow.Count + ctx.Db.SharedRow.Count; }";
        rootSource += $$"""
            public static class RootQueries {
                public static SpacetimeDB.IQuery<Alpha.AlphaRow> Alpha(SpacetimeDB.ViewContext ctx) => ctx.From.{{(mounted ? "Auth." : "")}}AlphaRow();
                public static SpacetimeDB.IQuery<Beta.BetaRow> Beta(SpacetimeDB.AnonymousViewContext ctx) => ctx.From.{{(mounted ? "@class." : "")}}BetaRow();
                public static SpacetimeDB.IQuery<Shared.SharedRow> Shared(SpacetimeDB.ViewContext ctx) => ctx.From.SharedRow();
            }
            """;
        var root = Generate(Create("Root", rootSource, beta, utility, shared, alpha));
        var reordered = Generate(Create("Root", rootSource, alpha, shared, utility, beta));
        Emit(root);
        Emit(reordered);

        string[] Calls(CSharpCompilation compilation) =>
            [.. Method(compilation, "Initialize").DescendantNodes()
                .OfType<InvocationExpressionSyntax>()
                .Select(call => call.Expression.ToString())
                .Where(call => call.EndsWith(".Register"))];
        CSharpCompilation[] orderedDependencies = mounted
            ? [sharedCompilation, alphaCompilation, betaCompilation]
            : [alphaCompilation, betaCompilation, sharedCompilation];
        var expected = new[] { Descriptor(root) + ".Register" }
            .Concat(orderedDependencies.Select(c => Descriptor(c) + ".Register")).ToArray();
        if (mounted)
        {
            var init = Method(root, "Initialize");
            var submodules = init.DescendantNodes().OfType<InvocationExpressionSyntax>()
                .Where(call => call.Expression.ToString().EndsWith(".RegisterSubmodule")).ToArray();
            Assert.Equal(["\"Auth\"", "\"class\""],
                submodules.Select(call => call.ArgumentList.Arguments[0].ToString()));
            var invalid = root.AddSyntaxTrees(CSharpSyntaxTree.ParseText(
                "public static class BadView { public static void Write(SpacetimeDB.ViewContext ctx) => ctx.Db.Auth.AlphaRow.Insert(new Alpha.AlphaRow()); }",
                fixture.ParseOptions));
            Assert.Contains(GetCompilationErrors(invalid), d => d.Id == "CS1061");
        }
        Assert.Equal(expected, Calls(root));
        Assert.Equal(expected, Calls(reordered));
        foreach (var (export, category) in new[]
        {
            ("__call_reducer__", "Reducer"),
            ("__call_procedure__", "Procedure"),
            ("__call_http_handler__", "HttpHandler"),
            ("__call_view__", "View"),
            ("__call_view_anon__", "AnonymousView"),
        })
        {
            var body = Method(root, export).Body!;
            var routes = body.DescendantNodes().OfType<InvocationExpressionSyntax>()
                .Where(call => call.Expression is MemberAccessExpressionSyntax member
                    && member.Name.Identifier.ValueText == "CallLocal" + category).ToArray();
            Assert.Equal(expected.Select(name => name.Replace(".Register", ".CallLocal" + category)),
                routes.Select(call => call.Expression.ToString()));
            Assert.All(routes, call => Assert.Equal("localId",
                call.ArgumentList.Arguments[0].Expression.ToString()));
            var offsets = body.DescendantNodes().OfType<AssignmentExpressionSyntax>()
                .Where(assignment => assignment.IsKind(SyntaxKind.SubtractAssignmentExpression));
            Assert.Equal(expected.Select(name => name.Replace(".Register", "." + category + "Count")),
                offsets.Select(assignment => assignment.Right.ToString()));
            var negativeGuard = Assert.IsType<IfStatementSyntax>(body.Statements[0]);
            Assert.Equal("id < 0", negativeGuard.Condition.ToString());
            Assert.Equal(Method(root, export).ToString(), Method(reordered, export).ToString());
        }
        Assert.DoesNotContain(Method(root, "Register").DescendantNodes()
            .OfType<InvocationExpressionSyntax>(), call =>
                call.Expression.ToString().Contains("AssemblyDescriptor.Register"));

        // An unrelated utility alone must not cause an otherwise empty module to register.
        var plainUtility = Emit(Create("PlainUtility", "public class PlainUtility { }"));
        var empty = Generate(Create("Empty", "", plainUtility));
        Assert.Empty(empty.GetSymbolsWithName("AssemblyDescriptor", SymbolFilter.Type));

        var nested = Emit(Generate(Create("Nested",
            "[assembly: SpacetimeDB.Namespace(typeof(Alpha.Sentinel), Accessor = \"Auth\")]",
            alpha, shared)));
        var nestedResult = CSharpGeneratorDriver.Create(
            [new Module().AsSourceGenerator()], parseOptions: fixture.ParseOptions
        ).RunGenerators(Create("Outer", "", nested, alpha, shared)).GetRunResult();
        Assert.Contains(nestedResult.Diagnostics, diagnostic =>
            diagnostic.GetMessage().Contains("Only the consuming root"));

        foreach (var kind in new[] { "Init", "ClientConnected", "ClientDisconnected" })
        {
            var lifecycle = Emit(Generate(Create("LifecycleDependency", $$"""
                public class Marker { }
                public static partial class LifecycleFunctions {
                    [SpacetimeDB.Reducer(SpacetimeDB.ReducerKind.{{kind}})]
                    public static void Handle(SpacetimeDB.ReducerContext ctx) { }
                }
                """)));
            var lifecycleResult = CSharpGeneratorDriver.Create(
                [new Module().AsSourceGenerator()], parseOptions: fixture.ParseOptions
            ).RunGenerators(Create("LifecycleConsumer",
                "[assembly: SpacetimeDB.Namespace(typeof(Marker), Accessor = \"Auth\")]",
                lifecycle)).GetRunResult();
            Assert.Contains(lifecycleResult.Diagnostics, diagnostic =>
                diagnostic.GetMessage().Contains("LifecycleFunctions.Handle (" + kind + ")")
                && diagnostic.GetMessage().Contains("Auth"));
            // The same dependency can still be published alone or merged into the root scope.
            Generate(Create("FlatLifecycleConsumer", "", lifecycle));
            Generate(Create("PublicLifecycleConsumer",
                "[assembly: SpacetimeDB.Namespace(typeof(Marker), Accessor = \"public\")]",
                lifecycle));
        }
    }

    [Fact]
    // A separately compiled consumer can call the descriptor, registration populates only the supplied builder, and two builders receive identical metadata without modifying the static root.
    public static async Task NamespaceDescriptorCrossAssemblyRegistration()
    {
        var fixture = await Fixture.Compile("server", "net10.0");
        var compilation = fixture.SampleCompilation;
        foreach (var generator in new IIncrementalGenerator[] { new Type(), new Module(), new EnvironmentGenerator() })
        {
            compilation = compilation.AddSyntaxTrees(
                fixture.RunGeneratorAndGetResult(generator).GeneratedTrees
            );
        }
        using var moduleDll = new MemoryStream();
        var moduleEmit = compilation.Emit(moduleDll);
        Assert.True(moduleEmit.Success, string.Join("\n", moduleEmit.Diagnostics));

        var moduleReference = MetadataReference.CreateFromImage(moduleDll.ToArray());
        var consumer = CSharpCompilation.Create(
            "DescriptorConsumer",
            references: fixture.SampleCompilation.References.Append(moduleReference),
            options: new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary)
        );
        var moduleAssembly = (IAssemblySymbol)consumer.GetAssemblyOrModuleSymbol(moduleReference)!;
        var markerType = consumer.GetTypeByMetadataName("SpacetimeDB.ModuleDescriptorAttribute");
        Assert.NotNull(markerType);
        var marker = Assert.Single(moduleAssembly.GetAttributes().Where(attribute =>
            SymbolEqualityComparer.Default.Equals(attribute.AttributeClass, markerType)
        ));
        var argument = Assert.Single(marker.ConstructorArguments);
        Assert.Equal(TypedConstantKind.Type, argument.Kind);
        var descriptor = Assert.IsAssignableFrom<INamedTypeSymbol>(argument.Value);
        Assert.Equal(TypeKind.Class, descriptor.TypeKind);
        Assert.True(descriptor.IsStatic);
        Assert.Equal(Accessibility.Public, descriptor.DeclaredAccessibility);
        Assert.Equal("AssemblyDescriptor", descriptor.Name);
        Assert.True(SymbolEqualityComparer.Default.Equals(
            moduleAssembly, descriptor.ContainingAssembly
        ));
        var descriptorName = descriptor.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat);
        var dispatchCalls = new List<string>();
        foreach (var category in new[] { "Reducer", "Procedure", "HttpHandler", "View", "AnonymousView" })
        {
            var methodName = "CallLocal" + category;
            var method = Assert.IsAssignableFrom<IMethodSymbol>(
                Assert.Single(descriptor.GetMembers(methodName))
            );
            Assert.Equal(Accessibility.Public, method.DeclaredAccessibility);
            Assert.True(method.IsStatic);
            Assert.DoesNotContain(method.GetAttributes(), attribute =>
                attribute.AttributeClass?.Name == "UnmanagedCallersOnlyAttribute");
            var count = Assert.IsAssignableFrom<IFieldSymbol>(
                Assert.Single(descriptor.GetMembers(category + "Count"))
            );
            Assert.True(count.IsConst);
            Assert.Equal(Accessibility.Public, count.DeclaredAccessibility);
            var localMethod = Assert.Single(compilation.SyntaxTrees
                .SelectMany(tree => tree.GetRoot().DescendantNodes())
                .OfType<MethodDeclarationSyntax>()
                .Where(method => method.Identifier.ValueText == methodName
                    && method.Parent is ClassDeclarationSyntax type
                    && type.Identifier.ValueText == "ModuleRegistration"));
            var localSwitch = Assert.IsType<SwitchExpressionSyntax>(localMethod.ExpressionBody!.Expression);
            Assert.Equal(localSwitch.Arms.Count - 1, Assert.IsType<int>(count.ConstantValue));

            var parameters = string.Join(", ", method.Parameters.Select(parameter =>
                parameter.Type.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat)
                + " " + parameter.Name));
            var arguments = string.Join(", ", method.Parameters.Select(parameter => parameter.Name));
            dispatchCalls.Add($"public static global::SpacetimeDB.Internal.Errno {methodName}({parameters})"
                + $" => {descriptorName}.{methodName}({arguments});");
        }

        var consumerSource = $$"""
            using System.IO;
            using System.Linq;
            using System.Reflection;
            using SpacetimeDB.Internal;

            public static class DescriptorConsumer
            {
                {{string.Join("\n", dispatchCalls)}}

                public static void Register(ModuleBuilder builder) =>
                    {{descriptorName}}.Register(builder);

                private static RawModuleDefV10 Describe(ModuleBuilder builder) =>
                    (RawModuleDefV10)typeof(ModuleBuilder).GetMethod(
                        "BuildModuleDefinition", BindingFlags.Instance | BindingFlags.NonPublic
                    )!.Invoke(builder, null)!;

                public static byte[] Snapshot(ModuleBuilder builder)
                {
                    using var stream = new MemoryStream();
                    using var writer = new BinaryWriter(stream);
                    new RawModuleDefV10.BSATN().Write(writer, Describe(builder));
                    return stream.ToArray();
                }

                public static string[] TableNames(ModuleBuilder builder) =>
                    Describe(builder).Sections.OfType<RawModuleDefV10Section.Tables>()
                        .SelectMany(section => section.Tables_)
                        .Select(table => table.SourceName).ToArray();

                public static string[] ReducerNames(ModuleBuilder builder) =>
                    Describe(builder).Sections.OfType<RawModuleDefV10Section.Reducers>()
                        .SelectMany(section => section.Reducers_)
                        .Select(reducer => reducer.SourceName).ToArray();

                public static string[] EnvironmentNames(ModuleBuilder builder) =>
                    Describe(builder).Sections.OfType<RawModuleDefV10Section.Environment>()
                        .SelectMany(section => section.Environment_)
                        .Select(declaration => declaration.Name).ToArray();

                public static bool CheckNamespaces()
                {
                    const string first = "Library, Version=1.0.0.0";
                    const string second = "Library, Version=2.0.0.0";
                    var placements = new System.Collections.Generic.Dictionary<string, string>
                    {
                        [first] = "auth_data",
                        [second] = "audit_data",
                        ["merged"] = "public",
                        [{{SymbolDisplay.FormatLiteral(moduleAssembly.Identity.ToString(), true)}}] = "cached",
                    };
                    var registry = new NamespaceRegistry("root", placements);
                    placements[first] = "changed";
                    if (registry.Resolve(first, "User") != "auth_data.User"
                        || registry.Resolve(second, "User") != "audit_data.User"
                        || registry.Resolve("root", "User") != "User"
                        || registry.Resolve("publicDependency", "User") != "User"
                        || registry.Resolve("merged", "User") != "User")
                        return false;
                    try { SpacetimeDB.Internal.Module.ResolveName(first, "User"); return false; }
                    catch (System.InvalidOperationException) { }
                    SpacetimeDB.Internal.Module.InstallNamespaces(registry);
                    if (SpacetimeDB.Internal.Module.ResolveName(first, "User") != "auth_data.User") return false;
                    if (SpacetimeDB.Internal.Module.ResolveSqlName(first, "User").ToString() != "\"auth_data\".\"User\"") return false;
                    if (registry.ResolveSqlName("publicDependency", "User").ToString() != "\"User\"") return false;
                    if (registry.ResolveSqlName("merged", "User").ToString() != "\"User\"") return false;
                    if (registry.ResolveSqlName("root", "User.With.Dot").ToString() != "\"User.With.Dot\"") return false;
                    try { SpacetimeDB.Internal.Module.InstallNamespaces(registry); return false; }
                    catch (System.InvalidOperationException) { }
                    try { new NamespaceRegistry(first, placements); return false; }
                    catch (System.ArgumentException) { }
                    try { new NamespaceRegistry("root", placements.Concat(placements)); return false; }
                    catch (System.ArgumentException) { }
                    return true;
                }
            }
            """;
        consumer = consumer.AddSyntaxTrees(
            CSharpSyntaxTree.ParseText(consumerSource, fixture.ParseOptions)
        );
        using var consumerDll = new MemoryStream();
        var consumerEmit = consumer.Emit(consumerDll);
        Assert.True(consumerEmit.Success, string.Join("\n", consumerEmit.Diagnostics));

        // Isolate the module's static root and Runtime types from other tests.
        var loadContext = new System.Runtime.Loader.AssemblyLoadContext(
            nameof(NamespaceDescriptorCrossAssemblyRegistration), isCollectible: true
        );
        loadContext.Resolving += (_, name) =>
        {
            if (!name.Name!.StartsWith("SpacetimeDB."))
            {
                return null;
            }
            var reference = fixture.SampleCompilation.References.Single(r =>
                fixture.SampleCompilation.GetAssemblyOrModuleSymbol(r)?.Name == name.Name
            );
            if (reference is CompilationReference projectReference)
            {
                using var implementation = new MemoryStream();
                var result = projectReference.Compilation.Emit(implementation);
                Assert.True(result.Success, string.Join("\n", result.Diagnostics));
                implementation.Position = 0;
                return loadContext.LoadFromStream(implementation);
            }
            var path = ((PortableExecutableReference)reference).FilePath!;
            // Reference assemblies cannot execute; use the sibling implementation assembly.
            var directory = new DirectoryInfo(Path.GetDirectoryName(path)!);
            if (directory.Name is "ref" or "refint")
            {
                path = Path.Combine(directory.Parent!.FullName, Path.GetFileName(path));
            }
            return loadContext.LoadFromAssemblyPath(path);
        };
        try
        {
            moduleDll.Position = 0;
            loadContext.LoadFromStream(moduleDll);
            consumerDll.Position = 0;
            var consumerType = loadContext.LoadFromStream(consumerDll)
                .GetType("DescriptorConsumer", throwOnError: true)!;
            var register = consumerType.GetMethod("Register")!;
            var builderType = register.GetParameters()[0].ParameterType;
            var first = Activator.CreateInstance(builderType)!;
            var second = Activator.CreateInstance(builderType)!;
            var root = builderType.Assembly.GetType("SpacetimeDB.Internal.Module")!
                .GetField("RootBuilder")!.GetValue(null)!;
            byte[] Snapshot(object builder) => (byte[])consumerType.GetMethod("Snapshot")!
                .Invoke(null, [builder])!;
            var empty = Snapshot(second);
            var rootBefore = Snapshot(root);

            register.Invoke(null, [first]);
            Assert.Equal<string>(["REQUIRED", "OPTIONAL", "MODE"], (string[])consumerType.GetMethod("EnvironmentNames")!
                .Invoke(null, [first])!);
            Assert.Empty((string[])consumerType.GetMethod("EnvironmentNames")!.Invoke(null, [root])!);
            Assert.Contains("PublicTable", (string[])consumerType.GetMethod("TableNames")!
                .Invoke(null, [first])!);
            Assert.Contains("InsertData", (string[])consumerType.GetMethod("ReducerNames")!
                .Invoke(null, [first])!);
            var registered = Snapshot(first);
            Assert.False(empty.SequenceEqual(registered));
            Assert.Equal(empty, Snapshot(second));
            Assert.Equal(rootBefore, Snapshot(root));

            register.Invoke(null, [second]);
            Assert.Equal(registered, Snapshot(second));
            Assert.Equal(registered, Snapshot(first));
            Assert.Equal(rootBefore, Snapshot(root));

            Assert.True((bool)consumerType.GetMethod("CheckNamespaces")!.Invoke(null, null)!);

            var loadedModule = loadContext.Assemblies.Single(a => a.GetName().Name == moduleAssembly.Name);
            var cachedHandles = loadedModule.GetTypes()
                .Select(type => (Type: type, Field: type.GetField("__resolvedName",
                    System.Reflection.BindingFlags.Static | System.Reflection.BindingFlags.NonPublic)))
                .Where(handle => handle.Field is not null).ToArray();
            Assert.NotEmpty(cachedHandles);
            foreach (var (type, field) in cachedHandles)
            {
                Assert.True(field!.IsInitOnly);
                Assert.False(type.Attributes.HasFlag(System.Reflection.TypeAttributes.BeforeFieldInit));
                var name = Assert.IsType<string>(field.GetValue(null));
                Assert.StartsWith("cached.", name);
                Assert.Same(name, field.GetValue(null));
            }
            foreach (var baseName in new[]
            {
                "UniqueIndex`4", "IndexBase`1",
                "ReadOnlyUniqueIndex`4", "ReadOnlyIndexBase`1",
                "ReadOnlyTableView`1",
            })
                Assert.Contains(cachedHandles, handle => handle.Type.BaseType!.Name == baseName);

            var queriesType = loadedModule.GetTypes().Single(type =>
                type.Name == "Queries" && type.DeclaringType?.Name == "AssemblyDescriptor");
            var queries = Activator.CreateInstance(queriesType);
            var factories = queriesType.GetMethods(System.Reflection.BindingFlags.Instance
                | System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.NonPublic
                | System.Reflection.BindingFlags.DeclaredOnly);
            Assert.NotEmpty(factories);
            var sqlNames = new List<string>();
            foreach (var factory in factories)
            {
                var cache = queriesType.DeclaringType!.GetNestedType(factory.Name + "SqlNameCache",
                    System.Reflection.BindingFlags.NonPublic)!;
                Assert.False(cache.Attributes.HasFlag(System.Reflection.TypeAttributes.BeforeFieldInit));
                var field = cache.GetField("Name", System.Reflection.BindingFlags.Static
                    | System.Reflection.BindingFlags.NonPublic)!;
                Assert.True(field.IsInitOnly);
                var cachedName = field.GetValue(null)!;
                var segments = field.FieldType.GetProperty("NamespaceSegments")!;
                var firstQuery = factory.Invoke(queries, null)!;
                var secondQuery = factory.Invoke(queries, null)!;
                Assert.NotSame(firstQuery, secondQuery);
                var nameField = factory.ReturnType.GetFields(System.Reflection.BindingFlags.Instance
                    | System.Reflection.BindingFlags.NonPublic).Single(f => f.FieldType == field.FieldType);
                Assert.Same(segments.GetValue(cachedName), segments.GetValue(nameField.GetValue(firstQuery)));
                Assert.Same(segments.GetValue(cachedName), segments.GetValue(nameField.GetValue(secondQuery)));
                Assert.Equal("SELECT * FROM " + cachedName,
                    factory.ReturnType.GetMethod("ToSql")!.Invoke(firstQuery, null));
                Assert.StartsWith("\"cached\".", cachedName.ToString());
                sqlNames.Add(cachedName.ToString()!);
            }
            Assert.Equal(factories.Length, sqlNames.Distinct().Count());
        }
        finally
        {
            loadContext.Unload();
        }
    }

    [Fact]
    public static async Task NamespaceEnvironmentAccessorsAreAssemblyLocal()
    {
        var fixture = await Fixture.Compile("server", "net10.0");
        const string usings = "global using System; global using System.IO; "
            + "global using System.Linq; global using System.Collections.Generic;\n#pragma warning disable STDB_UNSTABLE\n";
        CSharpCompilation Generate(string name, string source, params MetadataReference[] references)
        {
            var input = CSharpCompilation.Create(name,
                [CSharpSyntaxTree.ParseText(usings + source, fixture.ParseOptions)],
                fixture.SampleCompilation.References.Concat(references),
                new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary,
                    nullableContextOptions: NullableContextOptions.Enable));
            var driver = CSharpGeneratorDriver.Create(
                [new Type().AsSourceGenerator(), new Module().AsSourceGenerator(), new EnvironmentGenerator().AsSourceGenerator()],
                parseOptions: fixture.ParseOptions);
            driver.RunGeneratorsAndUpdateCompilation(input, out var output, out var diagnostics);
            Assert.Empty(diagnostics.Where(d => d.Severity == DiagnosticSeverity.Error));
            Assert.Empty(GetCompilationErrors(output));
            Assert.DoesNotContain(output.GetDiagnostics(), d => d.Id is "CS0433" or "CS0436");
            return (CSharpCompilation)output;
        }
        MetadataReference Emit(CSharpCompilation compilation)
        {
            using var dll = new MemoryStream();
            var result = compilation.Emit(dll);
            Assert.True(result.Success, string.Join("\n", result.Diagnostics));
            return MetadataReference.CreateFromImage(dll.ToArray());
        }

        var contexts = new[] { "ReducerContext", "ProcedureContext", "ProcedureTxContext",
            "HandlerContext", "HandlerTxContext", "ViewContext", "AnonymousViewContext" };
        var library = Generate("EnvironmentLibrary", """
            [SpacetimeDB.Env] public struct LibrarySchema { public string? SHARED; public string? LIBRARY_ONLY; }
            public static class LibraryHelpers {
                public static string? Read(SpacetimeDB.ReducerContext ctx) => ctx.Env.SHARED;
            }
            """);
        // An environment-only assembly must have a descriptor so it cannot disappear from discovery.
        Assert.Single(library.GetSymbolsWithName("AssemblyDescriptor", SymbolFilter.Type));
        var dependency = Emit(library);
        var root = Generate("EnvironmentRoot", """
            [SpacetimeDB.Env] public struct RootSchema { public string SHARED; public string? @class; }
            public static class RootHelpers {
                public static string? Library(SpacetimeDB.ReducerContext ctx) => LibraryHelpers.Read(ctx);
            }
            """ + string.Join("\n", contexts.Select((context, index) => $$"""
                public static class Context{{index}} {
                    public static string Read(SpacetimeDB.{{context}} ctx) => ctx.Env.SHARED;
                    public static string? Keyword(SpacetimeDB.{{context}} ctx) => ctx.Env.@class;
                    public static string? Generic(SpacetimeDB.{{context}} ctx) => ctx.Env.Get("LIBRARY_ONLY");
                }
                """)), dependency);
        Emit(root);
        var invalid = root.AddSyntaxTrees(CSharpSyntaxTree.ParseText("""
            public static class Invalid {
                public static string? Read(SpacetimeDB.ReducerContext ctx) => ctx.Env.LIBRARY_ONLY;
            }
            """, fixture.ParseOptions));
        Assert.Contains(GetCompilationErrors(invalid), d => d.Id == "CS1061");
        Assert.DoesNotContain(root.SyntaxTrees, tree => tree.ToString().Contains("ModuleInitializer"));
    }

    [Fact]
    // Can a separately compiled DLL accept our module’s contexts, and can module code still access tables through the contexts it returns?
    public static async Task NamespaceContextsCrossAssemblyBoundaries()
    {
        var fixture = await Fixture.Compile("server");
        var contextNames = new[]
        {
            "ReducerContext", "ProcedureContext", "ProcedureTxContext",
            "HandlerContext", "HandlerTxContext", "ViewContext", "AnonymousViewContext",
        };
        var helperSource = "#pragma warning disable STDB_UNSTABLE\npublic static class ContextHelpers {"
            + string.Join("\n", contextNames.Select(name =>
                $"public static SpacetimeDB.{name} Pass(SpacetimeDB.{name} ctx) => ctx;"))
            + "}";
        var helper = CSharpCompilation.Create(
            "ContextHelperLibrary",
            [CSharpSyntaxTree.ParseText(helperSource, fixture.ParseOptions)],
            fixture.SampleCompilation.References,
            new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary)
        );
        using var dll = new MemoryStream();
        var emitted = helper.Emit(dll);
        Assert.True(emitted.Success, string.Join("\n", emitted.Diagnostics));

        var compilation = fixture.SampleCompilation.AddReferences(
            MetadataReference.CreateFromImage(dll.ToArray())
        );
        foreach (var generator in new IIncrementalGenerator[] { new Type(), new Module(), new EnvironmentGenerator() })
        {
            compilation = compilation.AddSyntaxTrees(
                fixture.RunGeneratorAndGetResult(generator).GeneratedTrees
            );
        }
        var consumer = "#pragma warning disable STDB_UNSTABLE\n" + """
            public static class ContextConsumer
            {
                public static void Write(SpacetimeDB.ProcedureTxContext ctx) =>
                    ContextHelpers.Pass(ctx).Db.PublicTable.Insert(default);
                public static void Write(SpacetimeDB.HandlerTxContext ctx) =>
                    ContextHelpers.Pass(ctx).Db.PublicTable.Insert(default);
                public static ulong Read(SpacetimeDB.ViewContext ctx) =>
                    ContextHelpers.Pass(ctx).Db.PublicTable.Count;
                public static SpacetimeDB.IQuery<PublicTable> Query(SpacetimeDB.AnonymousViewContext ctx) =>
                    ContextHelpers.Pass(ctx).From.PublicTable();
            }
            """;
        consumer += "\npublic static class ContextIdentityConsumer {"
            + string.Join("\n", contextNames.Select(name =>
                $"public static SpacetimeDB.{name} Pass(SpacetimeDB.{name} ctx) => ContextHelpers.Pass(ctx);"))
            + "}";
        compilation = compilation.AddSyntaxTrees(CSharpSyntaxTree.ParseText(consumer, fixture.ParseOptions));
        Assert.Empty(GetCompilationErrors(compilation));
        AssertContextOwnership(compilation);

        var invalidWrites = CSharpSyntaxTree.ParseText("""
            public static class InvalidViewWrites
            {
                public static void Write(SpacetimeDB.ViewContext ctx) => ctx.Db.PublicTable.Insert(default);
                public static void Write(SpacetimeDB.AnonymousViewContext ctx) => ctx.Db.PublicTable.Insert(default);
            }
            """, fixture.ParseOptions);
        var errors = GetCompilationErrors(compilation.AddSyntaxTrees(invalidWrites)).ToArray();
        Assert.Equal(2, errors.Length);
        Assert.All(errors, error => Assert.Equal("CS1061", error.Id));
    }
#endif

    [Fact]
    public static async Task TypeAndModuleGeneratorsOnServer()
    {
        var fixture = await Fixture.Compile("server");
        await fixture.Verify(nameof(EnvironmentGenerator),
            fixture.RunGeneratorAndGetResult(new EnvironmentGenerator()));

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
