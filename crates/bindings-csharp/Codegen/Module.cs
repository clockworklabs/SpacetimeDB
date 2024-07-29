namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

[System.Flags]
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
            or SpecialType.System_UInt64
                => true,
            SpecialType.None => type.ToString() is "System.Int128" or "System.UInt128",
            _ => false
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
                    SpecialType.None
                        => type.ToString() is "SpacetimeDB.Address" or "SpacetimeDB.Identity",
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
        )
    ];

    public TableDeclaration(GeneratorAttributeSyntaxContext context)
        : base(context)
    {
        if (Kind is TypeKind.Sum)
        {
            throw new InvalidOperationException("Tagged enums cannot be tables.");
        }

        var attrArgs = context.Attributes.Single().NamedArguments;

        IsPublic = attrArgs.Any(pair => pair is { Key: "Public", Value.Value: true });

        Scheduled = attrArgs
            .Where(pair => pair.Key == "Scheduled")
            .Select(pair => (string?)pair.Value.Value)
            .SingleOrDefault();

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

        var hasAutoIncFields = Members.Any(f => f.Attrs.HasFlag(ColumnAttrs.AutoInc));

        var iTable = $"SpacetimeDB.Internal.ITable<{ShortName}>";

        // ITable inherits IStructuralReadWrite, so we can replace the base type instead of appending another one.
        extensions.BaseTypes.Clear();
        extensions.BaseTypes.Add(iTable);

        extensions.Contents.Append(
            $$"""
            static bool {{iTable}}.HasAutoIncFields => {{hasAutoIncFields.ToString().ToLower()}};

            static SpacetimeDB.Internal.Module.TableDesc {{iTable}}.MakeTableDesc(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                new (
                    nameof({{ShortName}}),
                    new SpacetimeDB.Internal.Module.ColumnDefWithAttrs[] {
                        {{string.Join(",\n", Members.Select(f => f.GenerateColumnDefWithAttrs()))}}
                    },
                    {{IsPublic.ToString().ToLower()}},
                    {{(Scheduled is not null ? $"\"{Scheduled}\"" : "null")}}
                ),
                (SpacetimeDB.BSATN.AlgebraicType.Ref) new BSATN().GetAlgebraicType(registrar)
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

    public KeyValuePair<string, string> GenerateClass()
    {
        var nonContextArgs = Args.Where(a => !a.IsContextArg);

        var class_ = $$"""
            class {{Name}}: SpacetimeDB.Internal.IReducer {
                {{string.Join(
                    "\n",
                    nonContextArgs.Select(a => a.GenerateBSATNField(Accessibility.Private))
                )}}

                public SpacetimeDB.Internal.Module.ReducerDef MakeReducerDef(SpacetimeDB.BSATN.ITypeRegistrar registrar) {
                    return new (
                        "{{ExportName}}",
                        {{MemberDeclaration.GenerateDefs(nonContextArgs)}}
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
            .Select((t, ct) => t.ToExtensions())
            .WithTrackingName("SpacetimeDB.Table.GenerateExtensions")
            .RegisterSourceOutputs(context);

        var tableNames = tables.Select((t, ct) => t.FullName).Collect();

        var addReducers = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: "SpacetimeDB.ReducerAttribute",
                predicate: (node, ct) => true, // already covered by attribute restrictions
                transform: (context, ct) => new ReducerDeclaration(context)
            )
            .WithTrackingName("SpacetimeDB.Reducer.Parse")
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
                        public static SpacetimeDB.Internal.Buffer __describe_module__() => SpacetimeDB.Internal.Module.__describe_module__();

                        [UnmanagedCallersOnly(EntryPoint = "__call_reducer__")]
                        public static SpacetimeDB.Internal.Buffer __call_reducer__(
                            uint id,
                            SpacetimeDB.Internal.Buffer caller_identity,
                            SpacetimeDB.Internal.Buffer caller_address,
                            SpacetimeDB.Internal.DateTimeOffsetRepr timestamp,
                            SpacetimeDB.Internal.Buffer args
                        ) => SpacetimeDB.Internal.Module.__call_reducer__(id, caller_identity, caller_address, timestamp, args);
                    #endif
                    }
                    """
                );
            }
        );
    }
}
