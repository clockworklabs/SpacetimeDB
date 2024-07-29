namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using System.Text;
using Microsoft.CodeAnalysis;
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

    public ColumnDeclaration(IFieldSymbol field)
    {
        Attrs = field
            .GetAttributes()
            .Where(a => a.AttributeClass?.ToString() == "SpacetimeDB.ColumnAttribute")
            .Select(a => (ColumnAttrs)a.ConstructorArguments[0].Value!)
            .SingleOrDefault();

        var type = field.Type;

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

        if (Attrs.HasFlag(ColumnAttrs.AutoInc) && !isInteger)
        {
            throw new Exception(
                $"{type} {Name} is not valid for AutoInc or Identity as it's not an integer."
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

        if (Attrs.HasFlag(ColumnAttrs.Unique) && !IsEquatable)
        {
            throw new Exception(
                $"{type} {Name} is not valid for Identity, PrimaryKey or PrimaryKeyAuto as it's not an equatable primitive."
            );
        }

        Name = field.Name;
        Type = SymbolToName(type);
        TypeInfo = GetTypeInfo(type);
    }

    // For the `TableDesc` constructor.
    public string GenerateColumnDefWithAttrs() =>
        $"""
            new (
                new (nameof({Name}), BSATN.{Name}.GetAlgebraicType(registrar)),
                SpacetimeDB.ColumnAttrs.{Attrs}
            )
            """;

    // For the `Filter` constructor.
    public string GenerateFilterEntry() =>
        $"new (nameof({Name}), (w, v) => BSATN.{Name}.Write(w, ({Type}) v!))";
}

record TableDeclaration
{
    public readonly Scope Scope;
    public readonly string ShortName;
    public readonly string FullName;
    public readonly EquatableArray<ColumnDeclaration> Fields;
    public readonly bool IsPublic;
    public readonly string? Scheduled;

    public TableDeclaration(GeneratorAttributeSyntaxContext context)
    {
        var tableSyntax = (TypeDeclarationSyntax)context.TargetNode;
        var table = (INamedTypeSymbol)context.TargetSymbol;
        var attrArgs = context.Attributes.Single().NamedArguments;

        IsPublic = attrArgs.Any(pair => pair is { Key: "Public", Value.Value: true });

        Scheduled = attrArgs
            .Where(pair => pair.Key == "Scheduled")
            .Select(pair => (string?)pair.Value.Value)
            .SingleOrDefault();

        var fields = GetFields(tableSyntax, table).Select(f => new ColumnDeclaration(f));

        if (Scheduled is not null)
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

        Scope = new Scope(tableSyntax);
        ShortName = table.Name;
        FullName = SymbolToName(table);
        Fields = new EquatableArray<ColumnDeclaration>(fields.ToImmutableArray());
    }

    public KeyValuePair<string, string> GenerateOutput()
    {
        var hasAutoIncFields = Fields.Any(f => f.Attrs.HasFlag(ColumnAttrs.AutoInc));

        var iTable = $"SpacetimeDB.Internal.ITable<{ShortName}>";

        var extensions = new StringBuilder();

        extensions.Append(
            $$"""
            static bool {{iTable}}.HasAutoIncFields => {{hasAutoIncFields.ToString().ToLower()}};

            static SpacetimeDB.Internal.Module.TableDesc {{iTable}}.MakeTableDesc(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                new (
                    nameof({{ShortName}}),
                    new SpacetimeDB.Internal.Module.ColumnDefWithAttrs[] {
                        {{string.Join(",\n", Fields.Select(f => f.GenerateColumnDefWithAttrs()))}}
                    },
                    {{IsPublic.ToString().ToLower()}},
                    {{(Scheduled is not null ? $"\"{Scheduled}\"" : "null")}}
                ),
                (SpacetimeDB.BSATN.AlgebraicType.Ref) new BSATN().GetAlgebraicType(registrar)
            );

            static SpacetimeDB.Internal.Filter {{iTable}}.CreateFilter() => new([
                {{string.Join(",\n", Fields.Select(f => f.GenerateFilterEntry()))}}
            ]);

            public static IEnumerable<{{ShortName}}> Iter() => {{iTable}}.Iter();
            public static IEnumerable<{{ShortName}}> Query(System.Linq.Expressions.Expression<Func<{{ShortName}}, bool>> predicate) => {{iTable}}.Query(predicate);
            public void Insert() => {{iTable}}.Insert(this);
            """
        );

        foreach (
            var (f, i) in Fields
                .Select((field, i) => (field, i))
                .Where(pair => pair.field.IsEquatable)
        )
        {
            var colEqWhere = $"{iTable}.ColEq.Where({i}, {f.Name}, BSATN.{f.Name})";

            extensions.Append(
                $"""
                public static IEnumerable<{ShortName}> FilterBy{f.Name}({f.Type} {f.Name}) =>
                    {colEqWhere}.Iter();
                """
            );

            if (f.Attrs.HasFlag(ColumnAttrs.Unique))
            {
                extensions.Append(
                    $"""
                    public static {ShortName}? FindBy{f.Name}({f.Type} {f.Name}) =>
                        FilterBy{f.Name}({f.Name})
                        .Cast<{ShortName}?>()
                        .SingleOrDefault();

                    public static bool DeleteBy{f.Name}({f.Type} {f.Name}) =>
                        {colEqWhere}.Delete();

                    public static bool UpdateBy{f.Name}({f.Type} {f.Name}, {ShortName} @this) =>
                        {colEqWhere}.Update(@this);
                    """
                );
            }
        }

        return new(FullName, Scope.GenerateExtensions(extensions.ToString(), iTable));
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

    public ReducerParamDeclaration(IParameterSymbol field)
        : this(field.Name, field.Type) { }
}

record ReducerDeclaration
{
    public readonly string Name;
    public readonly string ExportName;
    public readonly string FullName;
    public readonly EquatableArray<ReducerParamDeclaration> Args;
    public readonly Scope Scope;

    public ReducerDeclaration(GeneratorAttributeSyntaxContext context)
    {
        var methodSyntax = (MethodDeclarationSyntax)context.TargetNode;
        var method = (IMethodSymbol)context.TargetSymbol;
        var attr = context.Attributes.Single();

        if (!method.ReturnsVoid)
        {
            throw new Exception($"Reducer {method} must return void");
        }

        var exportName = (string?)attr.ConstructorArguments.SingleOrDefault().Value;

        Name = method.Name;
        ExportName = exportName ?? Name;
        FullName = SymbolToName(method);
        Args = new(
            method.Parameters.Select(p => new ReducerParamDeclaration(p)).ToImmutableArray()
        );
        Scope = new Scope(methodSyntax.Parent as MemberDeclarationSyntax);
    }

    private IEnumerable<ReducerParamDeclaration> NonContextArgs => Args.Where(a => !a.IsContextArg);

    public KeyValuePair<string, string> GenerateClass()
    {
        var class_ = $$"""
            class {{Name}}: SpacetimeDB.Internal.IReducer {
                {{string.Join(
                    "\n",
                    NonContextArgs.Select(a => $"private static {a.TypeInfo} {a.Name} = new();")
                )}}

                public SpacetimeDB.Internal.Module.ReducerDef MakeReducerDef(SpacetimeDB.BSATN.ITypeRegistrar registrar) {
                    return new (
                        "{{ExportName}}"
                        {{string.Join(
                            "",
                            NonContextArgs.Select(a => $",\nnew SpacetimeDB.BSATN.AggregateElement(nameof({a.Name}), {a.Name}.GetAlgebraicType(registrar))")
                        )}}
                    );
                }

                public void Invoke(BinaryReader reader, SpacetimeDB.ReducerContext ctx) {
                    {{FullName}}(
                        {{string.Join(
                            ", ",
                            Args.Select(a => a.IsContextArg ? "ctx" : $"{a.Name}.Read(reader)")
                        )}}
                    );
                }
            }
            """;

        return new(Name, class_);
    }

    public KeyValuePair<string, string> GenerateSchedule()
    {
        var method = $$"""
            public static void VolatileNonatomicScheduleImmediate{{Name}}({{string.Join(
                ", ",
                NonContextArgs.Select(a => $"{a.Type} {a.Name}")
            )}}) {
                using var stream = new MemoryStream();
                using var writer = new BinaryWriter(stream);
                {{string.Join(
                    "\n",
                    NonContextArgs.Select(a => $"new {a.TypeInfo}().Write(writer, {a.Name});")
                )}}
                SpacetimeDB.Internal.IReducer.VolatileNonatomicScheduleImmediate("{{ExportName}}", stream);
            }
            """;

        return new(FullName, Scope.GenerateExtensions(method));
    }
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
                transform: (context, ct) => new TableDeclaration(context)
            )
            .WithTrackingName("SpacetimeDB.Table.Parse");

        tables
            .Select((t, ct) => t.GenerateOutput())
            .WithTrackingName("SpacetimeDB.Table.GenerateExtensions")
            .RegisterSourceOutputs(context);

        var tableNames = tables.Select((t, ct) => t.FullName).Collect();

        var reducers = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: "SpacetimeDB.ReducerAttribute",
                predicate: (node, ct) => true, // already covered by attribute restrictions
                transform: (context, ct) => new ReducerDeclaration(context)
            )
            .WithTrackingName("SpacetimeDB.Reducer.Parse");

        reducers
            .Select((r, ct) => r.GenerateSchedule())
            .WithTrackingName("SpacetimeDB.Reducer.GenerateSchedule")
            .RegisterSourceOutputs(context);

        var addReducers = reducers
            .Select((r, ct) => r.GenerateClass())
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
                var addReducers = tuple.Right.Sort((a, b) => a.Key.CompareTo(b.Key));
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
                        {{string.Join("\n", addReducers.Select(r => r.Value))}}

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
                                    $"SpacetimeDB.Internal.Module.RegisterReducer<{r.Key}>();"
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
    }
}
