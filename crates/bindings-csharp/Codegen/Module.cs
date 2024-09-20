namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

record ColumnDeclaration : MemberDeclaration
{
    public readonly ImmutableArray<(string?, ColumnAttrs)> Attrs;
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
        (string?, ColumnAttrs)[] x = [(default(string), attrs)];
        Attrs = x.ToImmutableArray();
        IsEquatable = isEquatable;
    }

    public ColumnDeclaration(IFieldSymbol field)
        : base(field)
    {
        Attrs = field
            .GetAttributes()
            .Select(a => (a.NamedArguments.FirstOrDefault(a => a.Key == "Table").Value.Value as string,
                a.AttributeClass?.ToString() switch {
                    "SpacetimeDB.AutoIncAttribute" => ColumnAttrs.AutoInc,
                    "SpacetimeDB.PrimaryKeyAttribute" => ColumnAttrs.PrimaryKey,
                    "SpacetimeDB.UniqueAttribute" => ColumnAttrs.Unique,
                    "SpacetimeDB.IndexedAttribute" => ColumnAttrs.Indexed,
                    _ => ColumnAttrs.UnSet,
                }))
            .ToImmutableArray();

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

        var attrs = Attrs.Aggregate(ColumnAttrs.UnSet, (xs, x) => xs | x.Item2);

        if (attrs.HasFlag(ColumnAttrs.AutoInc) && !isInteger)
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

        if (attrs.HasFlag(ColumnAttrs.Unique) && !IsEquatable)
        {
            throw new Exception(
                $"{type} {Name} is not valid for Identity, PrimaryKey or PrimaryKeyAuto as it's not an equatable primitive."
            );
        }
    }

    public ColumnAttrs GetAttrs(string tableName) => Attrs
        .Where(x => x.Item1 == null || x.Item1 == tableName)
        .Aggregate(ColumnAttrs.UnSet, (xs, x) => xs | x.Item2);

    // For the `TableDesc` constructor.
    public string GenerateColumnDef() =>
        $"new (nameof({Name}), BSATN.{Name}.GetAlgebraicType(registrar))";

    // For the `Filter` constructor.
    public string GenerateFilterEntry() =>
        $"new (nameof({Name}), (w, v) => BSATN.{Name}.Write(w, ({Type}) v!))";
}

record TableView {
    public readonly TableDeclaration Table;
    public readonly string Name;
    public readonly bool IsPublic;
    public readonly string? Scheduled;

    public TableView(TableDeclaration table, AttributeData data) {
        Table = table;
        Name = data.NamedArguments.FirstOrDefault(x => x.Key == "Name").Value.Value as string ?? table.ShortName;

        IsPublic = data.NamedArguments.Any(pair => pair is { Key: "Public", Value.Value: true });

        Scheduled = data.NamedArguments
            .Where(pair => pair.Key == "Scheduled")
            .Select(pair => (string?)pair.Value.Value)
            .SingleOrDefault();
    }
}

record TableDeclaration : BaseTypeDeclaration<ColumnDeclaration>
{
    public readonly string Visibility;
    public readonly string? Scheduled;
    public readonly ImmutableArray<TableView> Views;

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

        Visibility = context.TargetSymbol.DeclaredAccessibility switch {
            Accessibility.ProtectedAndInternal
            or Accessibility.NotApplicable
            or Accessibility.Internal => "internal",
            Accessibility.Public => "public",
            _ => throw new Exception(
                "Table row type visibility must be public or internal."
            ),
        };

        Views = context.Attributes
            .Where(a => a.AttributeClass?.ToDisplayString() == "SpacetimeDB.TableAttribute")
            .Select(a => new TableView(this, a))
            .ToImmutableArray();

        var schedules = Views.Where(t => t.Scheduled != null).Select(t => t.Scheduled);
        var numSchedules = schedules.Count();
        if (numSchedules > 0) {
            var distinctSchedules = schedules.Distinct();
            if (numSchedules != Views.Length || distinctSchedules.Count() != 1) {
                throw new Exception("When using multiple [Table] attributes with schedule, all [Table] must have the same schedule.");
            }

            Scheduled = distinctSchedules.First();

            // For scheduled tables, we append extra fields early in the pipeline,
            // both to the type itself and to the BSATN information, as if they
            // were part of the original declaration.
            Members = new(Members.Concat(ScheduledColumns).ToImmutableArray());
        }
    }

    protected override ColumnDeclaration ConvertMember(IFieldSymbol field) => new(field);

    public IEnumerable<string> GenerateViewFilters(string viewName, string iTable) {
        foreach (
            var (f, i) in Members
                .Select((field, i) => (field, i))
                .Where(pair => pair.field.IsEquatable)
        ) {
            var colEqWhere = $"{iTable}.ColEq.Where({i}, {f.Name}, {FullName}.BSATN.{f.Name})";

            yield return $"""
                public IEnumerable<{FullName}> FilterBy{f.Name}({f.Type} {f.Name}) =>
                    {colEqWhere}.Iter();
                """;

            if (f.GetAttrs(viewName).HasFlag(ColumnAttrs.Unique)) {
                yield return $"""
                    public {FullName}? FindBy{f.Name}({f.Type} {f.Name}) =>
                        FilterBy{f.Name}({f.Name})
                        .Cast<{FullName}?>()
                        .SingleOrDefault();

                    public bool DeleteBy{f.Name}({f.Type} {f.Name}) =>
                        {colEqWhere}.Delete();

                    public bool UpdateBy{f.Name}({f.Type} {f.Name}, ref {FullName} @this) =>
                        {colEqWhere}.Update(ref @this);
                    """;
            }
        }
    }

    public IEnumerable<KeyValuePair<(string, string), (string, string)>> GenerateViews() {
        foreach (var v in Views) {
            var autoIncFields = Members
                .Where(f => f.GetAttrs(v.Name).HasFlag(ColumnAttrs.AutoInc))
                .Select(f => f.Name);

            var iTable = $"SpacetimeDB.Internal.ITableView<{v.Name}, {FullName}>";
            yield return new((v.Name, FullName), ($$"""
            {{Visibility}} readonly struct {{v.Name}} : {{iTable}} {
                static void {{iTable}}.ReadGenFields(System.IO.BinaryReader reader, ref {{FullName}} row) {
                    {{string.Join(
                        "\n",
                        autoIncFields.Select(name =>
                            $$"""
                            if (row.{{name}} == default)
                            {
                                row.{{name}} = {{FullName}}.BSATN.{{name}}.Read(reader);
                            }
                            """
                        )
                    )}}
                }
                public IEnumerable<{{FullName}}> Iter() => {{iTable}}.Iter();
                public IEnumerable<{{FullName}}> Query(System.Linq.Expressions.Expression<Func<{{FullName}}, bool>> predicate) => {{iTable}}.Query(predicate);
                public void Insert(ref {{FullName}} row) => {{iTable}}.Insert(ref row);
                {{string.Join("\n", GenerateViewFilters(v.Name, iTable))}}
            }
            """, $"{Visibility} TableViews.{v.Name} {v.Name} => new();"));
        }
    }

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

        var iTable = $"SpacetimeDB.Internal.ITable<{ShortName}>";

        // ITable inherits IStructuralReadWrite, so we can replace the base type instead of appending another one.
        extensions.BaseTypes.Clear();
        extensions.BaseTypes.Add(iTable);

        extensions.Contents.Append(
            $$"""
            static IEnumerable<SpacetimeDB.Internal.TableDesc> {{iTable}}.MakeTableDesc(SpacetimeDB.BSATN.ITypeRegistrar registrar) => [
            {{string.Join("\n", Views.Select(v => $$"""
            new (
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
                            .Select((col, pos) => (col, pos, col.GetAttrs(v.Name)))
                            .Where(tuple => tuple.Item3 != ColumnAttrs.UnSet)
                            .Select(pair =>
                                $$"""
                                new (
                                    nameof({{ShortName}}),
                                    {{pair.pos}},
                                    nameof({{pair.col.Name}}),
                                    (SpacetimeDB.ColumnAttrs){{(int)pair.Item3}}
                                )
                                """
                            )
                        )}}
                    ],
                    Sequences: [],
                    // "system" | "user"
                    TableType: "user",
                    // "public" | "private"
                    TableAccess: "{{(v.IsPublic ? "public" : "private")}}",
                    Scheduled: {{(v.Scheduled is not null ? $"nameof({v.Scheduled})" : "null")}}
                ),
                (uint) ((SpacetimeDB.BSATN.AlgebraicType.Ref) new BSATN().GetAlgebraicType(registrar)).Ref_
            ),
            """))}}
            ];

            static SpacetimeDB.Internal.Filter {{iTable}}.CreateFilter() => new([
                {{string.Join(",\n", Members.Select(f => f.GenerateFilterEntry()))}}
            ]);
            """
        );

        return extensions;
    }
}

record ReducerParamDeclaration : MemberDeclaration
{
    public ReducerParamDeclaration(IParameterSymbol param)
        : base(param.Name, param.Type)
    {
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
        if (method.Parameters.FirstOrDefault()?.Type is not INamedTypeSymbol namedType || namedType.Name != "ReducerContext") {
            throw new Exception($"Reducer {method} must have a first argument of type ReducerContext");
        }

        Name = method.Name;
        ExportName = attr.Name ?? Name;
        FullName = SymbolToName(method);
        Args = new(
            method.Parameters.Skip(1).Select(p => new ReducerParamDeclaration(p)).ToImmutableArray()
        );
        Scope = new Scope(methodSyntax.Parent as MemberDeclarationSyntax);
    }

    public KeyValuePair<string, string> GenerateClass()
    {
        var args = string.Join(", ", Args.Select(a => $"{a.Name}.Read(reader)"));
        var argsSep = args == "" ? "" : ", ";
        var class_ = $$"""
            class {{Name}}: SpacetimeDB.Internal.IReducer {
                {{MemberDeclaration.GenerateBsatnFields(Accessibility.Private, Args)}}

                public SpacetimeDB.Internal.ReducerDef MakeReducerDef(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                    "{{ExportName}}",
                    [{{MemberDeclaration.GenerateDefs(Args)}}]
                );

                public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx) {
                    {{FullName}}((SpacetimeDB.ReducerContext)ctx{{argsSep}}{{args}});
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
                Args.Select(a => $"{a.Type} {a.Name}")
            )}}) {
                using var stream = new MemoryStream();
                using var writer = new BinaryWriter(stream);
                {{string.Join(
                    "\n",
                    Args.Select(a => $"new {a.TypeInfo}().Write(writer, {a.Name});")
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

        var tableViews = tables
            .SelectMany((t, ct) => t.GenerateViews())
            .WithTrackingName("SpacetimeDB.Table.GenerateViews")
            .Collect();

        context.RegisterSourceOutput(
            tableViews.Combine(addReducers),
            (context, tuple) =>
            {
                // Sort tables and reducers by name to match Rust behaviour.
                // Not really important outside of testing, but for testing
                // it matters because we commit module-bindings
                // so they need to match 1:1 between different langs.
                var tableViews = tuple.Left.Sort((a, b) => a.Key.Item1.CompareTo(b.Key.Item1));
                var addReducers = tuple.Right.Sort((a, b) => a.Key.CompareTo(b.Key));
                // Don't generate the FFI boilerplate if there are no tables or reducers.
                if (tableViews.IsEmpty && addReducers.IsEmpty)
                    return;
                context.AddSource(
                    "FFI.cs",
                    $$"""
                    // <auto-generated />
                    #nullable enable

                    using System.Diagnostics.CodeAnalysis;
                    using System.Runtime.CompilerServices;
                    using System.Runtime.InteropServices;

                    namespace SpacetimeDB {
                        public sealed class ReducerContext : BaseReducerContext<Local> {}

                        namespace TableViews {
                            {{string.Join("\n", tableViews.Select(v => v.Value.Item1))}}
                        }

                        public sealed class Local {
                            {{string.Join("\n", tableViews.Select(v => v.Value.Item2))}}
                        }
                    }

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
                            SpacetimeDB.Internal.Module.Initialize(new SpacetimeDB.ReducerContext());

                            {{string.Join(
                                "\n",
                                addReducers.Select(r =>
                                    $"SpacetimeDB.Internal.Module.RegisterReducer<{r.Key}>();"
                                )
                            )}}
                            {{string.Join(
                                "\n",
                                tableViews.Select(t => $"SpacetimeDB.Internal.Module.RegisterTable<{t.Key.Item2}>();")
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
