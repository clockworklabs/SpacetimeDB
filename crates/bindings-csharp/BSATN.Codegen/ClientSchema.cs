namespace SpacetimeDB.Codegen;

using System.Text;
using System.Text.RegularExpressions;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

internal enum ClientFunctionKind
{
    Reducer,
    Procedure,
}

internal sealed record ClientFunction(
    IMethodSymbol Method,
    ClientFunctionKind Kind,
    string Name,
    string WireName,
    bool Invokable
);

internal sealed record ClientIndex(string Name, IFieldSymbol[] Fields, bool Unique);

internal sealed record ClientTable(
    string Accessor,
    string WireName,
    INamedTypeSymbol Row,
    IFieldSymbol[] Fields,
    IReadOnlyList<ClientIndex> Indexes,
    HashSet<string> IndexedColumns,
    string? PrimaryKey,
    bool Event,
    bool Parameterized
);

internal sealed class ClientScope(
    string directory,
    string ns,
    string connectionName,
    ClientScope? parent = null,
    string accessor = "",
    string wireName = ""
)
{
    public string Directory { get; } = directory;
    public string Namespace { get; } = ns;
    public string ConnectionName { get; } = connectionName;
    public ClientScope? Parent { get; } = parent;
    public string Accessor { get; } = accessor;
    public string MemberPath { get; } = parent == null ? "" : parent.MemberPath + accessor + ".";
    public string[] WireSegments { get; } =
        parent == null ? [] : [.. parent.WireSegments, wireName];

    public string QualifiedWireName(string name) => string.Join(".", WireSegments.Append(name));

    public List<ClientTable> Tables { get; } = [];
    public List<ClientFunction> Functions { get; } = [];
    public List<ClientScope> Children { get; } = [];
    public ClientScope Root => Parent?.Root ?? this;

    public IEnumerable<ClientScope> DescendantsAndSelf() =>
        new[] { this }.Concat(Children.SelectMany(child => child.DescendantsAndSelf()));
}

[Generator]
public sealed class ClientModule : IIncrementalGenerator
{
    private sealed record Wrapper(
        string Directory,
        string Namespace,
        string Name,
        Location Location
    );

    private static readonly DiagnosticDescriptor InvalidSchema =
        new(
            "STDBCLIENT001",
            "Invalid client module schema",
            "{0}",
            "SpacetimeDB.Client",
            DiagnosticSeverity.Error,
            true
        );

    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        var wrappers = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                "SpacetimeDB.ClientConnectionAttribute",
                static (node, _) => node is ClassDeclarationSyntax,
                static (attr, _) =>
                {
                    var file = attr.TargetNode.SyntaxTree.FilePath;
                    var folder = (string)attr.Attributes[0].ConstructorArguments[0].Value!;
                    var symbol = (INamedTypeSymbol)attr.TargetSymbol;
                    return new Wrapper(
                        FullPath(Path.Combine(Path.GetDirectoryName(file) ?? "", folder)),
                        symbol.ContainingNamespace.ToDisplayString(),
                        symbol.Name,
                        attr.TargetNode.GetLocation()
                    );
                }
            )
            .Collect();
        context.RegisterSourceOutput(
            wrappers.Combine(context.CompilationProvider),
            static (ctx, input) =>
            {
                var (roots, compilation) = input;
                foreach (var wrapper in roots)
                {
                    if (
                        roots.Any(other =>
                            other != wrapper
                            && (
                                Contains(wrapper.Directory, other.Directory)
                                || Contains(other.Directory, wrapper.Directory)
                            )
                        )
                    )
                    {
                        ctx.ReportDiagnostic(
                            Diagnostic.Create(
                                InvalidSchema,
                                wrapper.Location,
                                $"Connection '{wrapper.Namespace}.{wrapper.Name}' selects a schema that overlaps another connection."
                            )
                        );
                        continue;
                    }
                    var result = DiagReporter.With(
                        wrapper.Location,
                        diag => Generate(ctx, compilation, wrapper, diag)
                    );
                    foreach (var diagnostic in result.Diag)
                        ctx.ReportDiagnostic(diagnostic);
                    if (result.Parsed is { } source)
                        ctx.AddSource(
                            wrapper.Namespace + "." + wrapper.Name + ".Client.g.cs",
                            source
                        );
                }
            }
        );
    }

    private static string FullPath(string path) =>
        Path.GetFullPath(path).TrimEnd(Path.DirectorySeparatorChar);

    private static bool Contains(string directory, string path)
    {
        var comparison =
            Path.DirectorySeparatorChar == '\\'
                ? StringComparison.OrdinalIgnoreCase
                : StringComparison.Ordinal;
        return path.Equals(directory, comparison)
            || path.StartsWith(directory + Path.DirectorySeparatorChar, comparison);
    }

    private static string Generate(
        SourceProductionContext ctx,
        Compilation compilation,
        Wrapper wrapper,
        DiagReporter diag
    )
    {
        var root = new ClientScope(wrapper.Directory, wrapper.Namespace, wrapper.Name);
        var trees = compilation
            .SyntaxTrees.Where(t =>
                t.FilePath.Length > 0 && Contains(wrapper.Directory, FullPath(t.FilePath))
            )
            .OrderBy(t => t.FilePath, StringComparer.Ordinal)
            .ToArray();
        ResolveMounts(ctx, root, compilation, trees);

        var scopes = root.DescendantsAndSelf().OrderByDescending(s => s.Directory.Length).ToArray();
        var seenTypes = new HashSet<INamedTypeSymbol>(SymbolEqualityComparer.Default);
        var source = new StringBuilder(
            """
            // <auto-generated/>
            #nullable enable
            using System;
            using System.Collections.Generic;
            using System.Runtime.Serialization;
            using SpacetimeDB;
            using SpacetimeDB.ClientApi;
            """
        ).AppendLine();

        foreach (var tree in trees)
        {
            ctx.CancellationToken.ThrowIfCancellationRequested();
            var scope = scopes.First(s => Contains(s.Directory, FullPath(tree.FilePath)));
            var model = compilation.GetSemanticModel(tree);
            foreach (
                var node in tree.GetRoot(ctx.CancellationToken)
                    .DescendantNodes()
                    .OfType<MemberDeclarationSyntax>()
            )
            {
                if (
                    node is TypeDeclarationSyntax { AttributeLists.Count: > 0 } typeSyntax
                    && model.GetDeclaredSymbol(typeSyntax) is INamedTypeSymbol type
                    && seenTypes.Add(type)
                )
                {
                    ReadType(ctx, scope, source, type, diag);
                }
                else if (
                    node is MethodDeclarationSyntax { AttributeLists.Count: > 0 } syntax
                    && model.GetDeclaredSymbol(syntax) is IMethodSymbol method
                )
                {
                    ReadMethod(ctx, scope, source, syntax, method);
                }
            }
        }

        foreach (var scope in root.DescendantsAndSelf())
        {
            scope.Tables.Sort((a, b) => StringComparer.Ordinal.Compare(a.Accessor, b.Accessor));
            scope.Functions.Sort((a, b) => StringComparer.Ordinal.Compare(a.Name, b.Name));
            source.AppendLine($"namespace {scope.Namespace} {{");
            source.AppendLine(ClientContainers.Emit(scope));
            foreach (var table in scope.Tables)
                source.AppendLine(ClientTables.Emit(scope, table));
            foreach (var function in scope.Functions)
                source.AppendLine(ClientFunctions.Emit(scope, function, diag));
            if (scope.Parent == null)
                source.AppendLine(ClientConnectionEmitter.Emit(root));
            source.AppendLine("}");
        }
        return source.ToString();
    }

    private static void ResolveMounts(
        SourceProductionContext ctx,
        ClientScope root,
        Compilation compilation,
        SyntaxTree[] schemaTrees
    )
    {
        var mounts = compilation
            .Assembly.GetAttributes()
            .Where(attr =>
                attr.AttributeClass?.ToDisplayString() == "SpacetimeDB.NamespaceAttribute"
                && attr.ApplicationSyntaxReference is { } syntax
                && schemaTrees.Contains(syntax.SyntaxTree)
            )
            .ToList();

        // Mount ownership comes from the source of the assembly attribute and its marker, independently of CLR namespaces.
        void ReadChildren(ClientScope parent)
        {
            foreach (
                var mount in mounts
                    .Where(m =>
                        FullPath(
                            Path.GetDirectoryName(
                                m.ApplicationSyntaxReference!.SyntaxTree.FilePath
                            )!
                        ) == parent.Directory
                    )
                    .ToArray()
            )
            {
                mounts.Remove(mount);
                var marker = mount.ConstructorArguments[0].Value as INamedTypeSymbol;
                var directory = marker
                    ?.DeclaringSyntaxReferences.Select(r =>
                        Path.GetDirectoryName(r.SyntaxTree.FilePath)
                    )
                    .FirstOrDefault(d => d != null && Contains(root.Directory, FullPath(d)));
                var accessor = Option(mount, "Accessor") ?? "";
                if (
                    directory == null
                    || accessor.Length == 0
                    || root.DescendantsAndSelf().Any(s => s.Directory == FullPath(directory))
                )
                {
                    ctx.ReportDiagnostic(
                        Diagnostic.Create(
                            InvalidSchema,
                            mount.ApplicationSyntaxReference!.GetSyntax().GetLocation(),
                            $"Mount '{accessor}' must select a distinct schema directory within this connection."
                        )
                    );
                    continue;
                }
                var name = EscapeIdentifier(accessor);
                var child = new ClientScope(
                    FullPath(directory),
                    parent.Namespace + "." + name,
                    parent.ConnectionName,
                    parent,
                    name,
                    Option(mount, "Name") ?? SnakeCase(accessor)
                );
                parent.Children.Add(child);
                ReadChildren(child);
            }
        }
        ReadChildren(root);

        foreach (var mount in mounts)
            ctx.ReportDiagnostic(
                Diagnostic.Create(
                    InvalidSchema,
                    mount.ApplicationSyntaxReference!.GetSyntax().GetLocation(),
                    "The mount declaration must belong to its parent module's schema directory."
                )
            );
    }

    private static void ReadType(
        SourceProductionContext ctx,
        ClientScope scope,
        StringBuilder source,
        INamedTypeSymbol type,
        DiagReporter diag
    )
    {
        var tables = Attributes(type, "Table").ToArray();
        var annotation = Attribute(type, "Type") ?? tables.FirstOrDefault();
        if (
            annotation
                ?.ApplicationSyntaxReference?.GetSyntax()
                .FirstAncestorOrSelf<TypeDeclarationSyntax>()
            is not { } schema
        )
            return;
        if (
            !Contains(scope.Directory, FullPath(schema.SyntaxTree.FilePath))
            || tables.Any(t =>
                !Contains(
                    scope.Directory,
                    FullPath(t.ApplicationSyntaxReference!.SyntaxTree.FilePath)
                )
            )
        )
        {
            ctx.ReportDiagnostic(
                Diagnostic.Create(
                    InvalidSchema,
                    schema.GetLocation(),
                    $"The schema declarations of partial type '{type}' must belong to the same module."
                )
            );
            return;
        }
        source.AppendLine(ClientSchemaMembers.ForType(schema, type, diag));
        foreach (var table in tables)
            scope.Tables.Add(ClientTables.Parse(type, table));
    }

    private static void ReadMethod(
        SourceProductionContext ctx,
        ClientScope scope,
        StringBuilder source,
        MethodDeclarationSyntax syntax,
        IMethodSymbol method
    )
    {
        var reducer = Attribute(method, "Reducer");
        var procedure = Attribute(method, "Procedure");
        var view = Attribute(method, "View");
        var attribute = reducer ?? procedure ?? view;
        if (attribute == null)
            return;
        source.AppendLine(ClientSchemaMembers.ForMethod(syntax, method));
        var accessor = Option(attribute, "Accessor") ?? method.Name;
        var wireName = Option(attribute, "Name") ?? SnakeCase(accessor);
        if (view != null)
        {
            if (ClientTables.ParseView(method, view, accessor, wireName) is { } table)
                scope.Tables.Add(table);
            else
                ctx.ReportDiagnostic(
                    Diagnostic.Create(
                        InvalidSchema,
                        syntax.GetLocation(),
                        $"View '{method.Name}' must return a row or a collection of rows."
                    )
                );
            return;
        }

        // Init is invisible to clients. The connection lifecycle reducers have events only.
        var kindArg = reducer?.ConstructorArguments.FirstOrDefault();
        var kind = kindArg
            ?.Type?.GetMembers()
            .OfType<IFieldSymbol>()
            .FirstOrDefault(f => f.HasConstantValue && Equals(f.ConstantValue, kindArg.Value.Value))
            ?.Name;
        if (kind != "Init")
            scope.Functions.Add(
                new(
                    method,
                    reducer == null ? ClientFunctionKind.Procedure : ClientFunctionKind.Reducer,
                    accessor,
                    wireName,
                    kind is null or "UserDefined"
                )
            );
    }

    internal static IEnumerable<AttributeData> Attributes(ISymbol symbol, string name) =>
        symbol
            .GetAttributes()
            .Where(a => a.AttributeClass?.ToDisplayString() == "SpacetimeDB." + name + "Attribute");

    internal static AttributeData? Attribute(ISymbol symbol, string name) =>
        Attributes(symbol, name).FirstOrDefault();

    internal static string? Option(AttributeData attr, string key) =>
        attr.NamedArguments.FirstOrDefault(k => k.Key == key).Value.Value as string;

    internal static bool Flag(AttributeData attr, string key) =>
        attr.NamedArguments.FirstOrDefault(k => k.Key == key).Value.Value is true;

    internal static string Literal(string value) => SymbolDisplay.FormatLiteral(value, true);

    // Match the word boundaries used by the CLI for C# public names, including
    // acronym and digit boundaries (HTTP2Server -> http_2_server / Http2Server).
    private static readonly Regex WordBoundary =
        new(
            @"_+|(?<=[\p{Ll}])(?=[\p{Lu}])|(?<=[\p{Lu}])(?=[\p{Lu}][\p{Ll}])|(?<=\p{L})(?=\d)|(?<=\d)(?=\p{L})"
        );

    private static IEnumerable<string> Words(string name) =>
        WordBoundary
            .Split(name)
            .Where(word => word.Length != 0)
            .Select(word => word.ToLowerInvariant());

    internal static string SnakeCase(string name) => string.Join("_", Words(name));

    internal static string PascalCase(string name) =>
        string.Concat(Words(name).Select(word => char.ToUpperInvariant(word[0]) + word[1..]));
}
