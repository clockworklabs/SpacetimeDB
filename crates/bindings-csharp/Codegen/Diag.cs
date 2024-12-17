namespace SpacetimeDB.Codegen;

using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;

internal static class ErrorDescriptor
{
    private static readonly ErrorDescriptorGroup group = new("STDB", "SpacetimeDB");

    public static readonly ErrorDescriptor<MethodDeclarationSyntax> ReducerReturnType =
        new(
            group,
            "[SpacetimeDB.Reducer] methods must return void",
            method =>
                $"Reducer method {method.Identifier} returns {method.ReturnType} instead of void.",
            method => method.ReturnType
        );

    public static readonly ErrorDescriptor<IFieldSymbol> AutoIncNotInteger =
        new(
            group,
            "AutoInc fields must be of integer type",
            field =>
                $"Field {field.Name} is marked as AutoInc but it has a non-integer type {field.Type}.",
            field => field
        );

    public static readonly ErrorDescriptor<IFieldSymbol> UniqueNotEquatable =
        new(
            group,
            "Unique fields must be equatable",
            field =>
                $"Field {field.Name} is marked as Unique but it has a type {field.Type} which is not an equatable primitive.",
            field => field
        );

    public static readonly ErrorDescriptor<(
        ViewIndex index,
        AttributeData attr
    )> EmptyIndexColumns =
        new(
            group,
            "Index attribute must specify columns.",
            ctx =>
                $"{(ctx.index.AccessorName != "" ? ctx.index.AccessorName : "Index")} has an Index.BTree attribute, but no columns.",
            ctx => ctx.attr
        );

    public static readonly ErrorDescriptor<TypeDeclarationSyntax> InvalidTableVisibility =
        new(
            group,
            "Table row visibility must be public or internal, including container types.",
            table => $"Table {table.Identifier} and its parent types must be public or internal.",
            table => table.Identifier
        );

    public static readonly ErrorDescriptor<TypeDeclarationSyntax> TableTaggedEnum =
        new(
            group,
            "Tables cannot be tagged enums",
            table => $"Table {table.Identifier} is a tagged enum, which is not allowed.",
            table => table.BaseList!
        );

    public static readonly ErrorDescriptor<(
        string kind,
        string exportName,
        IEnumerable<string> fullNames
    )> DuplicateExport =
        new(
            group,
            "Duplicate exports",
            ctx =>
                $"{ctx.kind} with the same export name {ctx.exportName} is registered in multiple places: {string.Join(", ", ctx.fullNames)}",
            ctx => Location.None
        );

    public static readonly ErrorDescriptor<MethodDeclarationSyntax> ReducerContextParam =
        new(
            group,
            "Reducers must have a first argument of type ReducerContext",
            method =>
                $"Reducer method {method.Identifier} does not have a ReducerContext parameter.",
            method => method.ParameterList
        );

    public static readonly ErrorDescriptor<(
        MethodDeclarationSyntax method,
        string prefix
    )> ReducerReservedPrefix =
        new(
            group,
            "Reducer method has a reserved name prefix",
            ctx =>
                $"Reducer method {ctx.method.Identifier} starts with '{ctx.prefix}', which is a reserved prefix.",
            ctx => ctx.method.Identifier
        );

    public static readonly UnusedErrorDescriptor IncompatibleTableSchedule = new(group);

    public static readonly ErrorDescriptor<(
        ReducerKind kind,
        IEnumerable<string> fullNames
    )> DuplicateSpecialReducer =
        new(
            group,
            "Multiple reducers of the same kind",
            ctx =>
                $"Several reducers are assigned to the same lifecycle kind {ctx.kind}: {string.Join(", ", ctx.fullNames)}",
            ctx => Location.None
        );

    public static readonly ErrorDescriptor<(
        AttributeData attr,
        string message
    )> InvalidScheduledDeclaration =
        new(group, "Invalid scheduled table declaration", ctx => $"{ctx.message}", ctx => ctx.attr);
}
