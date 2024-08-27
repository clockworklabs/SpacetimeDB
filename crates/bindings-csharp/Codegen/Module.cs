namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

[Flags]
enum ColumnAttrs : byte
{
    UnSet = 0b0000,
    Indexed = 0b0001,
    AutoInc = 0b0010,
    Unique = Indexed | 0b0100,
    Identity = Unique | AutoInc,
    PrimaryKey = Unique | 0b1000,
    PrimaryKeyAuto = PrimaryKey | AutoInc,
}

readonly record struct ColumnDeclaration
{
    public readonly string Name;
    public readonly string Type;
    public readonly string TypeInfo;
    public readonly ColumnAttrs Attrs;
    public readonly bool IsEquatable;

    public ColumnDeclaration(
        string name,
        string type,
        string typeInfo,
        ColumnAttrs attrs,
        bool isEquatable
    )
    {
        Name = name;
        Type = type;
        TypeInfo = typeInfo;
        Attrs = attrs;
        IsEquatable = isEquatable;
    }

    public ColumnDeclaration(string name, ITypeSymbol type, ColumnAttrs attrs)
    {
        var isInteger = type.SpecialType switch
        {
            SpecialType.System_Byte
            or SpecialType.System_SByte
            or SpecialType.System_Int16
            or SpecialType.System_UInt16
            or SpecialType.System_Int32
            or SpecialType.System_UInt32
            or SpecialType.System_Int64
            or SpecialType.System_UInt64 => true,
            SpecialType.None => type.ToString()
                is "System.Int128"
                    or "System.UInt128"
                    or "SpacetimeDB.I128"
                    or "SpacetimeDB.U128"
                    or "SpacetimeDB.I256"
                    or "SpacetimeDB.U256",
            _ => false,
        };

        if (attrs.HasFlag(ColumnAttrs.AutoInc) && !isInteger)
        {
            throw new Exception(
                $"{type} {name} is not valid for AutoInc or Identity as it's not an integer."
            );
        }

        IsEquatable =
            (
                isInteger
                || type.SpecialType switch
                {
                    SpecialType.System_String or SpecialType.System_Boolean => true,
                    SpecialType.None => type.ToString()
                        is "SpacetimeDB.Address"
                            or "SpacetimeDB.Identity",
                    _ => false,
                }
            )
            && type.NullableAnnotation != NullableAnnotation.Annotated;

        if (attrs.HasFlag(ColumnAttrs.Unique) && !IsEquatable)
        {
            throw new Exception(
                $"{type} {name} is not valid for Identity, PrimaryKey or PrimaryKeyAuto as it's not an equatable primitive."
            );
        }

        Name = name;
        Type = SymbolToName(type);
        TypeInfo = GetTypeInfo(type);
        Attrs = attrs;
    }
}

record TableDeclaration
{
    public readonly Scope Scope;
    public readonly string ShortName;
    public readonly string FullName;
    public readonly EquatableArray<ColumnDeclaration> Fields;
    public readonly bool IsPublic;
    public readonly string? Scheduled;

    public TableDeclaration(
        TypeDeclarationSyntax tableSyntax,
        INamedTypeSymbol table,
        IEnumerable<ColumnDeclaration> fields,
        bool isPublic,
        string? scheduled
    )
    {
        Scope = new Scope(tableSyntax);
        ShortName = table.Name;
        FullName = SymbolToName(table);
        Fields = new EquatableArray<ColumnDeclaration>(fields.ToImmutableArray());
        IsPublic = isPublic;
        Scheduled = scheduled;
    }
}

readonly record struct ReducerParamDeclaration
{
    public readonly string Name;
    public readonly string Type;
    public readonly string TypeInfo;
    public readonly bool IsContextArg;

    public ReducerParamDeclaration(string name, ITypeSymbol type)
    {
        Name = name;
        Type = SymbolToName(type);
        TypeInfo = GetTypeInfo(type);
        IsContextArg = Type == "SpacetimeDB.ReducerContext";
    }
}

record ReducerDeclaration
{
    public readonly string Name;
    public readonly string ExportName;
    public readonly string FullName;
    public readonly EquatableArray<ReducerParamDeclaration> Args;
    public readonly Scope Scope;

    public ReducerDeclaration(
        MethodDeclarationSyntax methodSyntax,
        IMethodSymbol method,
        string? exportName
    )
    {
        Name = method.Name;
        ExportName = exportName ?? Name;
        FullName = SymbolToName(method);
        Args = new(
            method
                .Parameters.Select(p => new ReducerParamDeclaration(p.Name, p.Type))
                .ToImmutableArray()
        );
        Scope = new Scope(methodSyntax.Parent as MemberDeclarationSyntax);
    }

    public IEnumerable<ReducerParamDeclaration> GetNonContextArgs() =>
        Args.Where(a => !a.IsContextArg);
}

[Generator]
public class Module : IIncrementalGenerator
{
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        var tables = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: "SpacetimeDB.TableAttribute",
                predicate: (node, ct) => true, // already covered by attribute restrictions
                transform: (context, ct) =>
                {
                    var tableSyntax = (TypeDeclarationSyntax)context.TargetNode;
                    var table = context.SemanticModel.GetDeclaredSymbol(tableSyntax, ct)!;

                    var fields = GetFields(tableSyntax, table)
                        .Select(f =>
                        {
                            var indexKind = f.GetAttributes()
                                .Where(a =>
                                    a.AttributeClass?.ToString() == "SpacetimeDB.ColumnAttribute"
                                )
                                .Select(a => (ColumnAttrs)a.ConstructorArguments[0].Value!)
                                .SingleOrDefault();

                            return new ColumnDeclaration(f.Name, f.Type, indexKind);
                        });

                    var isPublic = context
                        .Attributes.SelectMany(attr => attr.NamedArguments)
                        .Any(pair => pair.Key == "Public" && pair.Value.Value is true);

                    var scheduled = context
                        .Attributes.SelectMany(attr => attr.NamedArguments)
                        .Where(pair => pair.Key == "Scheduled")
                        .Select(pair => (string?)pair.Value.Value)
                        .SingleOrDefault();

                    if (scheduled is not null)
                    {
                        // For scheduled tables, we append extra fields early in the pipeline,
                        // both to the type itself and to the BSATN information, as if they
                        // were part of the original declaration.
                        //
                        // TODO: simplify this when refactor for Table codegen inheriting Type
                        // codegen has landed (it's in WIP branch at the moment, sp meanwhile we
                        // need to do some logic duplication in both places).
                        fields = fields.Concat(
                            [
                                new(
                                    "ScheduledId",
                                    "ulong",
                                    "SpacetimeDB.BSATN.U64",
                                    ColumnAttrs.PrimaryKeyAuto,
                                    true
                                ),
                                new(
                                    "ScheduledAt",
                                    "SpacetimeDB.ScheduleAt",
                                    "SpacetimeDB.ScheduleAt.BSATN",
                                    ColumnAttrs.UnSet,
                                    false
                                ),
                            ]
                        );
                    }

                    return new TableDeclaration(tableSyntax, table, fields, isPublic, scheduled);
                }
            )
            .WithTrackingName("SpacetimeDB.Table.Parse");

        tables
            .Select(
                (t, ct) =>
                {
                    var autoIncFields = t
                        .Fields.Where(f => f.Attrs.HasFlag(ColumnAttrs.AutoInc))
                        .Select(f => f.Name);

                    var iTable = $"SpacetimeDB.Internal.ITable<{t.ShortName}>";

                    var extensions = new StringBuilder();

                    extensions.Append(
                        $$"""
                        static bool {{iTable}}.HasAutoIncFields => {{autoIncFields.Any().ToString().ToLower()}};

                        static SpacetimeDB.Internal.Module.TableDesc {{iTable}}.MakeTableDesc(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                            new (
                                nameof({{t.ShortName}}),
                                new SpacetimeDB.Internal.Module.ColumnDefWithAttrs[] { {{string.Join(",", t.Fields.Select(f =>
                                $"""
                                    new (
                                        new (nameof({f.Name}), BSATN.{f.Name}.GetAlgebraicType(registrar)),
                                        SpacetimeDB.ColumnAttrs.{f.Attrs}
                                    )
                                """
                                ))}} },
                                {{t.IsPublic.ToString().ToLower()}},
                                {{(t.Scheduled is not null ? $"\"{t.Scheduled}\"" : "null")}}
                            ),
                            (SpacetimeDB.BSATN.AlgebraicType.Ref) new BSATN().GetAlgebraicType(registrar)
                        );

                        static SpacetimeDB.Internal.Filter {{iTable}}.CreateFilter() => new([
                            {{string.Join("\n", t.Fields.Select(f => $"new (nameof({f.Name}), (w, v) => BSATN.{f.Name}.Write(w, ({f.Type}) v!)),"))}}
                        ]);

                        public static IEnumerable<{{t.ShortName}}> Iter() => {{iTable}}.Iter();
                        public static IEnumerable<{{t.ShortName}}> Query(System.Linq.Expressions.Expression<Func<{{t.ShortName}}, bool>> predicate) => {{iTable}}.Query(predicate);
                        public void Insert() => {{iTable}}.Insert(this);
                        """
                    );

                    foreach (
                        var (f, i) in t
                            .Fields.Select((field, i) => (field, i))
                            .Where(pair => pair.field.IsEquatable)
                    )
                    {
                        var colEqWhere = $"{iTable}.ColEq.Where({i}, {f.Name}, BSATN.{f.Name})";

                        extensions.Append(
                            $"""
                            public static IEnumerable<{t.ShortName}> FilterBy{f.Name}({f.Type} {f.Name}) =>
                                {colEqWhere}.Iter();
                            """
                        );

                        if (f.Attrs.HasFlag(ColumnAttrs.Unique))
                        {
                            extensions.Append(
                                $"""
                                public static {t.ShortName}? FindBy{f.Name}({f.Type} {f.Name}) =>
                                    FilterBy{f.Name}({f.Name})
                                    .Cast<{t.ShortName}?>()
                                    .SingleOrDefault();

                                public static bool DeleteBy{f.Name}({f.Type} {f.Name}) =>
                                    {colEqWhere}.Delete();

                                public static bool UpdateBy{f.Name}({f.Type} {f.Name}, {t.ShortName} @this) =>
                                    {colEqWhere}.Update(@this);
                                """
                            );
                        }
                    }

                    return new KeyValuePair<string, string>(
                        t.FullName,
                        t.Scope.GenerateExtensions(extensions.ToString(), iTable)
                    );
                }
            )
            .WithTrackingName("SpacetimeDB.Table.GenerateExtensions")
            .RegisterSourceOutputs(context);

        var tableNames = tables.Select((t, ct) => t.FullName).Collect();

        var reducers = context.SyntaxProvider.ForAttributeWithMetadataName(
            fullyQualifiedMetadataName: "SpacetimeDB.ReducerAttribute",
            predicate: (node, ct) => true, // already covered by attribute restrictions
            transform: (context, ct) =>
            {
                var methodSyntax = (MethodDeclarationSyntax)context.TargetNode;
                var method = context.SemanticModel.GetDeclaredSymbol(methodSyntax, ct)!;

                if (!method.ReturnsVoid)
                {
                    throw new Exception($"Reducer {method} must return void");
                }

                var exportName = (string?)
                    context
                        .Attributes.SingleOrDefault()
                        ?.ConstructorArguments.SingleOrDefault()
                        .Value;

                return new ReducerDeclaration(methodSyntax, method, exportName);
            }
        );

        var addReducers = reducers
            .WithTrackingName("SpacetimeDB.Reducer.Parse")
            .Select(
                (r, ct) =>
                    (
                        r.Name,
                        Class: $$"""
                        class {{r.Name}}: SpacetimeDB.Internal.IReducer {
                            {{string.Join(
                                "\n",
                                r.GetNonContextArgs()
                                    .Select(a => $"private static {a.TypeInfo} {a.Name} = new();")
                            )}}

                            public SpacetimeDB.Internal.Module.ReducerDef MakeReducerDef(SpacetimeDB.BSATN.ITypeRegistrar registrar) {
                                return new (
                                    "{{r.ExportName}}"
                                    {{string.Join(
                                        "",
                                        r.GetNonContextArgs()
                                            .Select(a =>
                                                $",\nnew SpacetimeDB.BSATN.AggregateElement(nameof({a.Name}), {a.Name}.GetAlgebraicType(registrar))"
                                            )
                                    )}}
                                );
                            }

                            public void Invoke(BinaryReader reader, SpacetimeDB.ReducerContext ctx) {
                                {{r.FullName}}({{string.Join(
                                    ", ",
                                    r.Args.Select(a => a.IsContextArg ? "ctx" : $"{a.Name}.Read(reader)")
                                )}});
                            }
                        }
                        """
                    )
            )
            .WithTrackingName("SpacetimeDB.Reducer.GenerateClass")
            .Collect();

        context.RegisterSourceOutput(
            tableNames.Combine(addReducers),
            (context, tuple) =>
            {
                // Sort tables and reducers by name to match Rust behaviour.
                // Not really important outside of testing, but for testing
                // it matters because we commit module-bindings
                // so they need to match 1:1 between different langs.
                var tableNames = tuple.Left.Sort();
                var addReducers = tuple.Right.Sort((a, b) => a.Name.CompareTo(b.Name));
                // Don't generate the FFI boilerplate if there are no tables or reducers.
                if (tableNames.IsEmpty && addReducers.IsEmpty)
                    return;
                context.AddSource(
                    "FFI.cs",
                    $$"""
                    // <auto-generated />
                    #nullable enable

                    using System.Diagnostics.CodeAnalysis;
                    using System.Runtime.CompilerServices;
                    using System.Runtime.InteropServices;

                    static class ModuleRegistration {
                        {{string.Join("\n", addReducers.Select(r => r.Class))}}

                    #if EXPERIMENTAL_WASM_AOT
                        // In AOT mode we're building a library.
                        // Main method won't be called automatically, so we need to export it as a preinit function.
                        [UnmanagedCallersOnly(EntryPoint = "__preinit__10_init_csharp")]
                    #else
                        // Prevent trimming of FFI exports that are invoked from C and not visible to C# trimmer.
                        [DynamicDependency(DynamicallyAccessedMemberTypes.PublicMethods, typeof(SpacetimeDB.Internal.Module))]
                    #endif
                        public static void Main() {
                            {{string.Join(
                                "\n",
                                addReducers.Select(r =>
                                    $"SpacetimeDB.Internal.Module.RegisterReducer<{r.Name}>();"
                                )
                            )}}
                            {{string.Join(
                                "\n",
                                tableNames.Select(t => $"SpacetimeDB.Internal.Module.RegisterTable<{t}>();")
                            )}}
                        }

                    // Exports only work from the main assembly, so we need to generate forwarding methods.
                    #if EXPERIMENTAL_WASM_AOT
                        [UnmanagedCallersOnly(EntryPoint = "__describe_module__")]
                        public static void __describe_module__(SpacetimeDB.Internal.BytesSink d) => SpacetimeDB.Internal.Module.__describe_module__(d);

                        [UnmanagedCallersOnly(EntryPoint = "__call_reducer__")]
                        public static short __call_reducer__(
                            uint id,
                            ulong sender_0,
                            ulong sender_1,
                            ulong sender_2,
                            ulong sender_3,
                            ulong address_0,
                            ulong address_1,
                            SpacetimeDB.Internal.DateTimeOffsetRepr timestamp,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink error
                        ) => SpacetimeDB.Internal.Module.__call_reducer__(
                            id,
                            sender_0,
                            sender_1,
                            sender_2,
                            sender_3,
                            address_0,
                            address_0,
                            timestamp,
                            args,
                            error
                        );
                    #endif
                    }
                    """
                );
            }
        );

        reducers
            .Select(
                (r, ct) =>
                    new KeyValuePair<string, string>(
                        r.FullName,
                        r.Scope.GenerateExtensions(
                            $@"
                            public static void VolatileNonatomicScheduleImmediate{r.Name}({string.Join(", ", r.GetNonContextArgs().Select(a => $"{a.Type} {a.Name}"))}) {{
                                using var stream = new MemoryStream();
                                using var writer = new BinaryWriter(stream);
                                {string.Join("\n", r.GetNonContextArgs().Select(a => $"new {a.TypeInfo}().Write(writer, {a.Name});"))}
                                SpacetimeDB.Internal.IReducer.VolatileNonatomicScheduleImmediate(""{r.ExportName}"", stream);
                            }}
                        "
                        )
                    )
            )
            .WithTrackingName("SpacetimeDB.Reducer.GenerateSchedule")
            .RegisterSourceOutputs(context);
    }
}
