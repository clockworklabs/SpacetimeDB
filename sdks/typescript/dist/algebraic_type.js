"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.AlgebraicType = exports.BuiltinType = exports.MapType = exports.ProductType = exports.ProductTypeElement = exports.SumType = exports.SumTypeVariant = void 0;
/**
 * A variant of a sum type.
 *
 * NOTE: Each element has an implicit element tag based on its order.
 * Uniquely identifies an element similarly to protobuf tags.
 */
class SumTypeVariant {
    name;
    algebraicType;
    constructor(name, algebraicType) {
        this.name = name;
        this.algebraicType = algebraicType;
    }
}
exports.SumTypeVariant = SumTypeVariant;
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
class SumType {
    variants;
    constructor(variants) {
        this.variants = variants;
    }
}
exports.SumType = SumType;
/**
* A factor / element of a product type.
*
* An element consist of an optional name and a type.
*
* NOTE: Each element has an implicit element tag based on its order.
* Uniquely identifies an element similarly to protobuf tags.
*/
class ProductTypeElement {
    name;
    algebraicType;
    constructor(name, algebraicType) {
        this.name = name;
        this.algebraicType = algebraicType;
    }
}
exports.ProductTypeElement = ProductTypeElement;
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
class ProductType {
    elements;
    constructor(elements) {
        this.elements = elements;
    }
    isEmpty() {
        return this.elements.length === 0;
    }
}
exports.ProductType = ProductType;
/* A map type from keys of type `keyType` to values of type `valueType`. */
class MapType {
    keyType;
    valueType;
    constructor(keyType, valueType) {
        this.keyType = keyType;
        this.valueType = valueType;
    }
}
exports.MapType = MapType;
class BuiltinType {
    type;
    arrayType;
    mapType;
    constructor(type, arrayOrMapType) {
        this.type = type;
        if (arrayOrMapType !== undefined) {
            if (arrayOrMapType.constructor === MapType) {
                this.mapType = arrayOrMapType;
            }
            else if (arrayOrMapType.constructor === AlgebraicType) {
                this.arrayType = arrayOrMapType;
            }
        }
    }
}
exports.BuiltinType = BuiltinType;
// exporting BuiltinType as a namespace as well as a class allows to add
// export types on the namespace, so we can use BuiltinType.Type
/*
* Represents the built-in types in SATS.
*
* Some of these types are nominal in our otherwise structural type system.
*/
(function (BuiltinType) {
    let Type;
    (function (Type) {
        Type["Bool"] = "Bool";
        Type["I8"] = "I8";
        Type["U8"] = "U8";
        Type["I16"] = "I16";
        Type["U16"] = "U16";
        Type["I32"] = "I32";
        Type["U32"] = "U32";
        Type["I64"] = "I64";
        Type["U64"] = "U64";
        Type["I128"] = "I128";
        Type["U128"] = "U128";
        Type["F32"] = "F32";
        Type["F64"] = "F64";
        /** UTF-8 encoded */
        Type["String"] = "String";
        /** This is a SATS `ArrayType`
          *
          * An array type is a **homogeneous** product type of dynamic length.
          *
          * That is, it is a product type
          * where every element / factor / field is of the same type
          * and where the length is statically unknown.
         */
        Type["Array"] = "Array";
        /** This is a SATS `MapType` */
        Type["Map"] = "Map";
    })(Type = BuiltinType.Type || (BuiltinType.Type = {}));
})(BuiltinType = exports.BuiltinType || (exports.BuiltinType = {}));
/**
* The SpacetimeDB Algebraic Type System (SATS) is a structural type system in
* which a nominal type system can be constructed.
*
* The type system unifies the concepts sum types, product types, and built-in
* primitive types into a single type system.
*/
class AlgebraicType {
    type;
    type_;
    get product() {
        if (this.type !== Type.ProductType) {
            throw "product type was requested, but the type is not ProductType";
        }
        return this.type_;
    }
    set product(value) {
        this.type_ = value;
        this.type = value == undefined ? Type.None : Type.ProductType;
    }
    get sum() {
        if (this.type !== Type.SumType) {
            throw "sum type was requested, but the type is not SumType";
        }
        return this.type_;
    }
    set sum(value) {
        this.type_ = value;
        this.type = value == undefined ? Type.None : Type.SumType;
    }
    get builtin() {
        if (this.type !== Type.BuiltinType) {
            throw "builtin type was requested, but the type is not BuiltinType";
        }
        return this.type_;
    }
    set builtin(value) {
        this.type_ = value;
        this.type = value == undefined ? Type.None : Type.BuiltinType;
    }
    static createProductType(elements) {
        let type = new AlgebraicType();
        type.product = new ProductType(elements);
        return type;
    }
    static createArrayType(elementType) {
        let type = new AlgebraicType();
        type.builtin = new BuiltinType(BuiltinType.Type.Array, elementType);
        return type;
    }
    static createSumType(variants) {
        let type = new AlgebraicType();
        type.sum = new SumType(variants);
        return type;
    }
    static createPrimitiveType(type) {
        let algebraicType = new AlgebraicType();
        algebraicType.builtin = new BuiltinType(type, undefined);
        return algebraicType;
    }
    isProductType() {
        return this.type === Type.ProductType;
    }
    isSumType() {
        return this.type === Type.SumType;
    }
}
exports.AlgebraicType = AlgebraicType;
(function (AlgebraicType) {
    let Type;
    (function (Type) {
        Type["SumType"] = "SumType";
        Type["ProductType"] = "ProductType";
        Type["BuiltinType"] = "BuiltinType";
        Type["None"] = "None";
    })(Type = AlgebraicType.Type || (AlgebraicType.Type = {}));
})(AlgebraicType = exports.AlgebraicType || (exports.AlgebraicType = {}));
let Type = AlgebraicType.Type;
