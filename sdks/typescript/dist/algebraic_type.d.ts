/**
 * A variant of a sum type.
 *
 * NOTE: Each element has an implicit element tag based on its order.
 * Uniquely identifies an element similarly to protobuf tags.
 */
export declare class SumTypeVariant {
    name: string;
    algebraicType: AlgebraicType;
    constructor(name: string, algebraicType: AlgebraicType);
}
/**
* Unlike most languages, sums in SATS are *[structural]* and not nominal.
* When checking whether two nominal types are the same,
* their names and/or declaration sites (e.g., module / namespace) are considered.
* Meanwhile, a structural type system would only check the structure of the type itself,
* e.g., the names of its variants and their inner data types in the case of a sum.
*
* This is also known as a discriminated union (implementation) or disjoint union.
* Another name is [coproduct (category theory)](https://ncatlab.org/nlab/show/coproduct).
*
* These structures are known as sum types because the number of possible values a sum
* ```ignore
* { N_0(T_0), N_1(T_1), ..., N_n(T_n) }
* ```
* is:
* ```ignore
* Σ (i ∈ 0..n). values(T_i)
* ```
* so for example, `values({ A(U64), B(Bool) }) = values(U64) + values(Bool)`.
*
* See also: https://ncatlab.org/nlab/show/sum+type.
*
* [structural]: https://en.wikipedia.org/wiki/Structural_type_system
*/
export declare class SumType {
    variants: SumTypeVariant[];
    constructor(variants: SumTypeVariant[]);
}
/**
* A factor / element of a product type.
*
* An element consist of an optional name and a type.
*
* NOTE: Each element has an implicit element tag based on its order.
* Uniquely identifies an element similarly to protobuf tags.
*/
export declare class ProductTypeElement {
    name: string;
    algebraicType: AlgebraicType;
    constructor(name: string, algebraicType: AlgebraicType);
}
/**
* A structural product type  of the factors given by `elements`.
*
* This is also known as `struct` and `tuple` in many languages,
* but note that unlike most languages, products in SATs are *[structural]* and not nominal.
* When checking whether two nominal types are the same,
* their names and/or declaration sites (e.g., module / namespace) are considered.
* Meanwhile, a structural type system would only check the structure of the type itself,
* e.g., the names of its fields and their types in the case of a record.
* The name "product" comes from category theory.
*
* See also: https://ncatlab.org/nlab/show/product+type.
*
* These structures are known as product types because the number of possible values in product
* ```ignore
* { N_0: T_0, N_1: T_1, ..., N_n: T_n }
* ```
* is:
* ```ignore
* Π (i ∈ 0..n). values(T_i)
* ```
* so for example, `values({ A: U64, B: Bool }) = values(U64) * values(Bool)`.
*
* [structural]: https://en.wikipedia.org/wiki/Structural_type_system
*/
export declare class ProductType {
    elements: ProductTypeElement[];
    constructor(elements: ProductTypeElement[]);
    isEmpty(): boolean;
}
export declare class MapType {
    keyType: AlgebraicType;
    valueType: AlgebraicType;
    constructor(keyType: AlgebraicType, valueType: AlgebraicType);
}
export declare class BuiltinType {
    type: BuiltinType.Type;
    arrayType: AlgebraicType | undefined;
    mapType: MapType | undefined;
    constructor(type: BuiltinType.Type, arrayOrMapType: AlgebraicType | MapType | undefined);
}
export declare namespace BuiltinType {
    enum Type {
        Bool = "Bool",
        I8 = "I8",
        U8 = "U8",
        I16 = "I16",
        U16 = "U16",
        I32 = "I32",
        U32 = "U32",
        I64 = "I64",
        U64 = "U64",
        I128 = "I128",
        U128 = "U128",
        F32 = "F32",
        F64 = "F64",
        /** UTF-8 encoded */
        String = "String",
        /** This is a SATS `ArrayType`
          *
          * An array type is a **homogeneous** product type of dynamic length.
          *
          * That is, it is a product type
          * where every element / factor / field is of the same type
          * and where the length is statically unknown.
         */
        Array = "Array",
        /** This is a SATS `MapType` */
        Map = "Map"
    }
}
type TypeRef = null;
type None = null;
export type EnumLabel = {
    label: string;
};
type AnyType = ProductType | SumType | BuiltinType | EnumLabel | TypeRef | None;
/**
* The SpacetimeDB Algebraic Type System (SATS) is a structural type system in
* which a nominal type system can be constructed.
*
* The type system unifies the concepts sum types, product types, and built-in
* primitive types into a single type system.
*/
export declare class AlgebraicType {
    type: Type;
    type_?: AnyType;
    get product(): ProductType;
    set product(value: ProductType | undefined);
    get sum(): SumType;
    set sum(value: SumType | undefined);
    get builtin(): BuiltinType;
    set builtin(value: BuiltinType | undefined);
    static createProductType(elements: ProductTypeElement[]): AlgebraicType;
    static createArrayType(elementType: AlgebraicType): AlgebraicType;
    static createSumType(variants: SumTypeVariant[]): AlgebraicType;
    static createPrimitiveType(type: BuiltinType.Type): AlgebraicType;
    isProductType(): boolean;
    isSumType(): boolean;
}
export declare namespace AlgebraicType {
    enum Type {
        SumType = "SumType",
        ProductType = "ProductType",
        BuiltinType = "BuiltinType",
        None = "None"
    }
}
type Type = AlgebraicType.Type;
declare let Type: typeof AlgebraicType.Type;
export {};
