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

    public static readonly ErrorDescriptor<TypeDeclarationSyntax> TableTaggedEnum =
        new(
            group,
            "Tables cannot be tagged enums",
            table => $"Table {table.Identifier} is a tagged enum, which is not allowed.",
            table => table.BaseList!
        );
}
