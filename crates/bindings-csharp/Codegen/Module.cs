namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

record ColumnDeclaration : MemberDeclaration
{
    public readonly ColumnAttrs Attrs;
    public readonly bool IsEquatable;

    public ColumnDeclaration(
        string name,
        string type,
        string typeInfo,
        ColumnAttrs attrs,
        bool isEquatable
    )
        : base(name, type, typeInfo)
    {
        Attrs = attrs;
        IsEquatable = isEquatable;
    }

    public ColumnDeclaration(IFieldSymbol field)
        : base(field)
    {
        Attrs = field
            .GetAttributes()
            .Where(a => a.AttributeClass?.ToString() == typeof(ColumnAttribute).FullName)
            .Select(a => a.ParseAs<ColumnAttribute>().Type)
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
    }

    // For the `TableDesc` constructor.
    public string GenerateColumnDef() =>
        $"new (nameof({Name}), BSATN.{Name}.GetAlgebraicType(registrar))";

    // For the `Filter` constructor.
    public string GenerateFilterEntry() =>
        $"new (nameof({Name}), (w, v) => BSATN.{Name}.Write(w, ({Type}) v!))";
}

record TableDeclaration : BaseTypeDeclaration<ColumnDeclaration>
{
    public readonly bool IsPublic;
    public readonly string? Scheduled;

    private static readonly ColumnDeclaration[] ScheduledColumns =
    [
        new("ScheduledId", "ulong", "SpacetimeDB.BSATN.U64", ColumnAttrs.PrimaryKeyAuto, true),
        new(
            "ScheduledAt",
            "SpacetimeDB.ScheduleAt",
            "SpacetimeDB.ScheduleAt.BSATN",
            ColumnAttrs.UnSet,
            false
        ),
    ];

    public TableDeclaration(GeneratorAttributeSyntaxContext context)
        : base(context)
    {
        if (Kind is TypeKind.Sum)
        {
            throw new InvalidOperationException("Tagged enums cannot be tables.");
        }

        var attr = context.Attributes.Single().ParseAs<TableAttribute>();

        IsPublic = attr.Public;
        Scheduled = attr.Scheduled;

        if (Scheduled is not null)
        {
            // For scheduled tables, we append extra fields early in the pipeline,
            // both to the type itself and to the BSATN information, as if they
            // were part of the original declaration.
            Members = new(Members.Concat(ScheduledColumns).ToImmutableArray());
        }
    }

    protected override ColumnDeclaration ConvertMember(IFieldSymbol field) => new(field);

    public override Scope.Extensions ToExtensions()
    {
        var extensions = base.ToExtensions();

        if (Scheduled is not null)
        {
            // For scheduled tables we're adding extra fields to the type source.
            extensions.Contents.Append(
                $$"""
                public ulong ScheduledId;
                public SpacetimeDB.ScheduleAt ScheduledAt;
                """
            );

            // When doing so, the compiler will warn about undefined ordering between partial declarations.
            // We don't care about ordering as we generate BSATN ourselves and don't use those structs in FFI,
            // so we can safely suppress the warning by saying "yes, we're okay with an auto/arbitrary layout".
            extensions.ExtraAttrs.Add(
                "[System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Auto)]"
            );
        }

        var autoIncFields = Members
            .Where(f => f.Attrs.HasFlag(ColumnAttrs.AutoInc))
            .Select(f => f.Name);

        var iTable = $"SpacetimeDB.Internal.ITable<{ShortName}>";

        // ITable inherits IStructuralReadWrite, so we can replace the base type instead of appending another one.
        extensions.BaseTypes.Clear();
        extensions.BaseTypes.Add(iTable);

        extensions.Contents.Append(
            $$"""
            void {{iTable}}.ReadGenFields(System.IO.BinaryReader reader) {
                {{string.Join(
                    "\n",
                    autoIncFields.Select(name =>
                        $$"""
                        if ({{name}} == default)
                        {
                            {{name}} = BSATN.{{name}}.Read(reader);
                        }
                        """
                    )
                )}}
            }

            static SpacetimeDB.Internal.TableDesc {{iTable}}.MakeTableDesc(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                new (
                    TableName: nameof({{ShortName}}),
                    Columns: [
                        {{string.Join(",\n", Members.Select(m => m.GenerateColumnDef()))}}
                    ],
                    Indexes: [],
                    Constraints: [
                        {{string.Join(
                            ",\n",
                            Members
                            // Important: the position must be stored here, before filtering.
                            .Select((col, pos) => (col, pos))
                            .Where(pair => pair.col.Attrs != ColumnAttrs.UnSet)
                            .Select(pair =>
                                $$"""
                                new (
                                    nameof({{ShortName}}),
                                    {{pair.pos}},
                                    nameof({{pair.col.Name}}),
                                    SpacetimeDB.ColumnAttrs.{{pair.col.Attrs}}
                                )
                                """
                            )
                        )}}
                    ],
                    Sequences: [],
                    // "system" | "user"
                    TableType: "user",
                    // "public" | "private"
                    TableAccess: "{{(IsPublic ? "public" : "private")}}",
                    Scheduled: {{(Scheduled is not null ? $"nameof({Scheduled})" : "null")}}
                ),
                (uint) ((SpacetimeDB.BSATN.AlgebraicType.Ref) new BSATN().GetAlgebraicType(registrar)).Ref_
            );

            static SpacetimeDB.Internal.Filter {{iTable}}.CreateFilter() => new([
                {{string.Join(",\n", Members.Select(f => f.GenerateFilterEntry()))}}
            ]);

            public static IEnumerable<{{ShortName}}> Iter() => {{iTable}}.Iter();
            public static IEnumerable<{{ShortName}}> Query(System.Linq.Expressions.Expression<Func<{{ShortName}}, bool>> predicate) => {{iTable}}.Query(predicate);
            public void Insert() => {{iTable}}.Insert(this);
            """
        );

        foreach (
            var (f, i) in Members
                .Select((field, i) => (field, i))
                .Where(pair => pair.field.IsEquatable)
        )
        {
            var colEqWhere = $"{iTable}.ColEq.Where({i}, {f.Name}, BSATN.{f.Name})";

            extensions.Contents.Append(
                $"""
                public static IEnumerable<{ShortName}> FilterBy{f.Name}({f.Type} {f.Name}) =>
                    {colEqWhere}.Iter();
                """
            );

            if (f.Attrs.HasFlag(ColumnAttrs.Unique))
            {
                extensions.Contents.Append(
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

        return extensions;
    }
}

record ReducerParamDeclaration : MemberDeclaration
{
    public readonly bool IsContextArg;

    public ReducerParamDeclaration(IParameterSymbol param)
        : base(param.Name, param.Type)
    {
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

    public ReducerDeclaration(GeneratorAttributeSyntaxContext context)
    {
        var methodSyntax = (MethodDeclarationSyntax)context.TargetNode;
        var method = (IMethodSymbol)context.TargetSymbol;
        var attr = context.Attributes.Single().ParseAs<ReducerAttribute>();

        if (!method.ReturnsVoid)
        {
            throw new Exception($"Reducer {method} must return void");
        }

        Name = method.Name;
        ExportName = attr.Name ?? Name;
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
                {{MemberDeclaration.GenerateBsatnFields(Accessibility.Private, NonContextArgs)}}

                public SpacetimeDB.Internal.ReducerDef MakeReducerDef(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                    "{{ExportName}}",
                    [{{MemberDeclaration.GenerateDefs(NonContextArgs)}}]
                );

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

    public Scope.Extensions GenerateSchedule()
    {
        var extensions = new Scope.Extensions(Scope, FullName);

        extensions.Contents.Append(
            $$"""
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
            """
        );

        return extensions;
    }
}

[Generator]
public class Module : IIncrementalGenerator
{
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        var tables = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: typeof(TableAttribute).FullName,
                predicate: (node, ct) => true, // already covered by attribute restrictions
                transform: (context, ct) => new TableDeclaration(context)
            )
            .WithTrackingName("SpacetimeDB.Table.Parse");

        tables
            .Select((t, ct) => t.ToExtensions())
            .WithTrackingName("SpacetimeDB.Table.GenerateExtensions")
            .RegisterSourceOutputs(context);

        var tableNames = tables.Select((t, ct) => t.FullName).Collect();

        var reducers = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: typeof(ReducerAttribute).FullName,
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
                {
                    return;
                }
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
                        public static SpacetimeDB.Internal.Errno __call_reducer__(
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
                            address_1,
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
