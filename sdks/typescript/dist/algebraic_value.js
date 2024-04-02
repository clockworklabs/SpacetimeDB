"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.AlgebraicValue = exports.BuiltinValue = exports.ProductValue = exports.SumValue = exports.JSONAdapter = exports.BinaryAdapter = exports.BinaryReducerArgsAdapter = exports.JSONReducerArgsAdapter = void 0;
const algebraic_type_1 = require("./algebraic_type");
class JSONReducerArgsAdapter {
    args;
    index = 0;
    constructor(args) {
        this.args = args;
    }
    next() {
        if (this.index >= this.args.length) {
            throw "Number of arguments in the reducer is larger than what we got from the server";
        }
        const adapter = new JSONAdapter(this.args[this.index]);
        this.index += 1;
        return adapter;
    }
}
exports.JSONReducerArgsAdapter = JSONReducerArgsAdapter;
class BinaryReducerArgsAdapter {
    adapter;
    constructor(adapter) {
        this.adapter = adapter;
    }
    next() {
        return this.adapter;
    }
}
exports.BinaryReducerArgsAdapter = BinaryReducerArgsAdapter;
class BinaryAdapter {
    reader;
    constructor(reader) {
        this.reader = reader;
    }
    callMethod(methodName) {
        return this[methodName]();
    }
    readUInt8Array() {
        const length = this.reader.readU32();
        return this.reader.readUInt8Array(length);
    }
    readArray(type) {
        const length = this.reader.readU32();
        let result = [];
        for (let i = 0; i < length; i++) {
            result.push(AlgebraicValue.deserialize(type, this));
        }
        return result;
    }
    readMap(keyType, valueType) {
        const mapLength = this.reader.readU32();
        let result = new Map();
        for (let i = 0; i < mapLength; i++) {
            const key = AlgebraicValue.deserialize(keyType, this);
            const value = AlgebraicValue.deserialize(valueType, this);
            result.set(key, value);
        }
        return result;
    }
    readString() {
        const strLength = this.reader.readU32();
        return this.reader.readString(strLength);
    }
    readSum(type) {
        let tag = this.reader.readByte();
        let sumValue = AlgebraicValue.deserialize(type.variants[tag].algebraicType, this);
        return new SumValue(tag, sumValue);
    }
    readProduct(type) {
        let elements = [];
        for (let element of type.elements) {
            elements.push(AlgebraicValue.deserialize(element.algebraicType, this));
        }
        return new ProductValue(elements);
    }
    readBool() {
        return this.reader.readBool();
    }
    readByte() {
        return this.reader.readByte();
    }
    readI8() {
        return this.reader.readI8();
    }
    readU8() {
        return this.reader.readU8();
    }
    readI16() {
        return this.reader.readI16();
    }
    readU16() {
        return this.reader.readU16();
    }
    readI32() {
        return this.reader.readI32();
    }
    readU32() {
        return this.reader.readU32();
    }
    readI64() {
        return this.reader.readI64();
    }
    readU64() {
        return this.reader.readU64();
    }
    readU128() {
        return this.reader.readU128();
    }
    readI128() {
        return this.reader.readI128();
    }
    readF32() {
        return this.reader.readF32();
    }
    readF64() {
        return this.reader.readF64();
    }
}
exports.BinaryAdapter = BinaryAdapter;
class JSONAdapter {
    value;
    constructor(value) {
        this.value = value;
    }
    callMethod(methodName) {
        return this[methodName]();
    }
    readUInt8Array() {
        return Uint8Array.from(this.value.match(/.{1,2}/g).map((byte) => parseInt(byte, 16)));
    }
    readArray(type) {
        let result = [];
        for (let el of this.value) {
            result.push(AlgebraicValue.deserialize(type, new JSONAdapter(el)));
        }
        return result;
    }
    readMap(_keyType, _valueType) {
        let result = new Map();
        // for (let i = 0; i < this.value.length; i++) {
        //   const key = AlgebraicValue.deserialize(
        //     keyType,
        //     new JSONAdapter()
        //   );
        //   const value = AlgebraicValue.deserialize(
        //     valueType,
        //     this
        //   );
        //   result.set(key, value);
        // }
        //
        return result;
    }
    readString() {
        return this.value;
    }
    readSum(type) {
        let tag = parseInt(Object.keys(this.value)[0]);
        let variant = type.variants[tag];
        let enumValue = Object.values(this.value)[0];
        let sumValue = AlgebraicValue.deserialize(variant.algebraicType, new JSONAdapter(enumValue));
        return new SumValue(tag, sumValue);
    }
    readProduct(type) {
        let elements = [];
        for (let i in type.elements) {
            let element = type.elements[i];
            elements.push(AlgebraicValue.deserialize(element.algebraicType, new JSONAdapter(this.value[i])));
        }
        return new ProductValue(elements);
    }
    readBool() {
        return this.value;
    }
    readByte() {
        return this.value;
    }
    readI8() {
        return this.value;
    }
    readU8() {
        return this.value;
    }
    readI16() {
        return this.value;
    }
    readU16() {
        return this.value;
    }
    readI32() {
        return this.value;
    }
    readU32() {
        return this.value;
    }
    readI64() {
        return this.value;
    }
    readU64() {
        return this.value;
    }
    readU128() {
        return this.value;
    }
    readI128() {
        return this.value;
    }
    readF32() {
        return this.value;
    }
    readF64() {
        return this.value;
    }
}
exports.JSONAdapter = JSONAdapter;
/** A value of a sum type choosing a specific variant of the type. */
class SumValue {
    /** A tag representing the choice of one variant of the sum type's variants. */
    tag;
    /**
    * Given a variant `Var(Ty)` in a sum type `{ Var(Ty), ... }`,
    * this provides the `value` for `Ty`.
    */
    value;
    constructor(tag, value) {
        this.tag = tag;
        this.value = value;
    }
    static deserialize(type, adapter) {
        if (type === undefined) {
            // TODO: get rid of undefined here
            throw "sum type is undefined";
        }
        return adapter.readSum(type);
    }
}
exports.SumValue = SumValue;
/**
* A product value is made of a list of
* "elements" / "fields" / "factors" of other `AlgebraicValue`s.
*
* The type of product value is a [product type](`ProductType`).
*/
class ProductValue {
    elements;
    constructor(elements) {
        this.elements = elements;
    }
    static deserialize(type, adapter) {
        if (type === undefined) {
            throw "type is undefined";
        }
        return adapter.readProduct(type);
    }
}
exports.ProductValue = ProductValue;
class BuiltinValue {
    value;
    constructor(value) {
        this.value = value;
    }
    static deserialize(type, adapter) {
        switch (type.type) {
            case algebraic_type_1.BuiltinType.Type.Array:
                let arrayBuiltinType = type.arrayType &&
                    type.arrayType.type === algebraic_type_1.AlgebraicType.Type.BuiltinType
                    ? type.arrayType.builtin.type
                    : undefined;
                if (arrayBuiltinType !== undefined &&
                    arrayBuiltinType === algebraic_type_1.BuiltinType.Type.U8) {
                    const value = adapter.readUInt8Array();
                    return new this(value);
                }
                else {
                    const arrayResult = adapter.readArray(type.arrayType);
                    return new this(arrayResult);
                }
            case algebraic_type_1.BuiltinType.Type.Map:
                let keyType = type.mapType.keyType;
                let valueType = type.mapType.valueType;
                const mapResult = adapter.readMap(keyType, valueType);
                return new this(mapResult);
            case algebraic_type_1.BuiltinType.Type.String:
                const result = adapter.readString();
                return new this(result);
            default:
                const methodName = "read" + type.type;
                return new this(adapter.callMethod(methodName));
        }
    }
    asString() {
        return this.value;
    }
    asArray() {
        return this.value;
    }
    asJsArray(type) {
        return this.asArray().map((el) => el.callMethod(("as" + type)));
    }
    asNumber() {
        return this.value;
    }
    asBool() {
        return this.value;
    }
    asBigInt() {
        return this.value;
    }
    asBoolean() {
        return this.value;
    }
    asBytes() {
        return this.value;
    }
}
exports.BuiltinValue = BuiltinValue;
/** A value in SATS. */
class AlgebraicValue {
    /** A structural sum value. */
    sum;
    /** A structural product value. */
    product;
    /** A builtin value that has a builtin type */
    builtin;
    constructor(value) {
        if (value === undefined) {
            // TODO: possibly get rid of it
            throw "value is undefined";
        }
        switch (value.constructor) {
            case SumValue:
                this.sum = value;
                break;
            case ProductValue:
                this.product = value;
                break;
            case BuiltinValue:
                this.builtin = value;
                break;
        }
    }
    callMethod(methodName) {
        return this[methodName]();
    }
    static deserialize(type, adapter) {
        switch (type.type) {
            case algebraic_type_1.AlgebraicType.Type.ProductType:
                return new this(ProductValue.deserialize(type.product, adapter));
            case algebraic_type_1.AlgebraicType.Type.SumType:
                return new this(SumValue.deserialize(type.sum, adapter));
            case algebraic_type_1.AlgebraicType.Type.BuiltinType:
                return new this(BuiltinValue.deserialize(type.builtin, adapter));
            default:
                throw new Error("not implemented");
        }
    }
    asProductValue() {
        if (!this.product) {
            throw "AlgebraicValue is not a ProductValue and product was requested";
        }
        return this.product;
    }
    asBuiltinValue() {
        this.assertBuiltin();
        return this.builtin;
    }
    asSumValue() {
        if (!this.sum) {
            throw "AlgebraicValue is not a SumValue and a sum value was requested";
        }
        return this.sum;
    }
    asArray() {
        this.assertBuiltin();
        return this.builtin.asArray();
    }
    asString() {
        this.assertBuiltin();
        return this.builtin.asString();
    }
    asNumber() {
        this.assertBuiltin();
        return this.builtin.asNumber();
    }
    asBool() {
        this.assertBuiltin();
        return this.builtin.asBool();
    }
    asBigInt() {
        this.assertBuiltin();
        return this.builtin.asBigInt();
    }
    asBoolean() {
        this.assertBuiltin();
        return this.builtin.asBool();
    }
    asBytes() {
        this.assertBuiltin();
        return this.builtin.asBytes();
    }
    assertBuiltin() {
        if (!this.builtin) {
            throw "AlgebraicValue is not a BuiltinValue and a string was requested";
        }
    }
}
exports.AlgebraicValue = AlgebraicValue;
