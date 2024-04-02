"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.BinarySerializer = exports.JSONSerializer = void 0;
const algebraic_type_1 = require("./algebraic_type");
const binary_writer_1 = __importDefault(require("./binary_writer"));
const identity_1 = require("./identity");
class JSONSerializer {
    content;
    index = 0;
    constructor() {
        this.content = [];
    }
    args() {
        return this.content;
    }
    serializeBuiltinType(type, value) {
        switch (type.type) {
            case algebraic_type_1.BuiltinType.Type.Array:
                const returnArray = [];
                for (const element of value) {
                    returnArray.push(this.serializeType(type.arrayType, element));
                }
                return returnArray;
            case algebraic_type_1.BuiltinType.Type.Map:
                break;
            default:
                return value;
        }
    }
    serializeType(type, value) {
        switch (type.type) {
            case algebraic_type_1.AlgebraicType.Type.BuiltinType:
                return this.serializeBuiltinType(type.builtin, value);
            case algebraic_type_1.AlgebraicType.Type.ProductType:
                let serializedArray = [];
                for (const element of type.product.elements) {
                    let serialized;
                    // If the value is an identity we can't use the `[]` operator, so we're
                    // special casing the identity type. It might be possible to define the
                    // `__identity_bytes` property on Identity, but I don't have time to check
                    // at the moment
                    if (value.constructor === identity_1.Identity) {
                        serialized = value.toHexString();
                    }
                    else {
                        serialized = this.serializeType(element.algebraicType, value[element.name]);
                    }
                    serializedArray.push(serialized);
                }
                return serializedArray;
            case algebraic_type_1.AlgebraicType.Type.SumType:
                if (type.sum.variants.length == 2 &&
                    type.sum.variants[0].name === "some" &&
                    type.sum.variants[1].name === "none") {
                    return value;
                }
                else {
                    const variant = type.sum.variants.find((v) => v.name === value.tag);
                    if (!variant) {
                        throw `Can't serialize a sum type, couldn't find ${value.tag} tag`;
                    }
                    return this.serializeType(variant.algebraicType, value.value);
                }
            default:
                break;
        }
    }
    write(type, value) {
        this.content[this.index] = this.serializeType(type, value);
        this.index += 1;
    }
}
exports.JSONSerializer = JSONSerializer;
class BinarySerializer {
    writer;
    constructor() {
        this.writer = new binary_writer_1.default(1024);
    }
    args() {
        return this.getBuffer();
    }
    getBuffer() {
        return this.writer.getBuffer();
    }
    write(type, value) {
        switch (type.type) {
            case algebraic_type_1.AlgebraicType.Type.BuiltinType:
                this.writeBuiltinType(type.builtin, value);
                break;
            case algebraic_type_1.AlgebraicType.Type.ProductType:
                for (const element of type.product.elements) {
                    this.write(element.algebraicType, 
                    // If the value is an Identity we have to return an Uint8Array instead of trying
                    // to use the `[]` operator.
                    value.constructor === identity_1.Identity
                        ? value.toUint8Array()
                        : value[element.name]);
                }
                break;
            case algebraic_type_1.AlgebraicType.Type.SumType:
                if (type.sum.variants.length == 2 &&
                    type.sum.variants[0].name === "some" &&
                    type.sum.variants[1].name === "none") {
                    if (value) {
                        this.writeByte(0);
                        this.write(type.sum.variants[0].algebraicType, value);
                    }
                    else {
                        this.writeByte(1);
                    }
                }
                else {
                    const index = type.sum.variants.findIndex((v) => v.name === value.tag);
                    if (index < 0) {
                        throw `Can't serialize a sum type, couldn't find ${value.tag} tag`;
                    }
                    this.writeByte(index);
                    this.write(type.sum.variants[index].algebraicType, value.value);
                }
                break;
            default:
                break;
        }
    }
    writeBuiltinType(type, value) {
        switch (type.type) {
            case algebraic_type_1.BuiltinType.Type.Array:
                const array = value;
                this.writer.writeU32(array.length);
                for (const element of array) {
                    this.write(type.arrayType, element);
                }
                break;
            case algebraic_type_1.BuiltinType.Type.Map:
                break;
            case algebraic_type_1.BuiltinType.Type.String:
                this.writer.writeString(value);
                break;
            case algebraic_type_1.BuiltinType.Type.Bool:
                this.writer.writeBool(value);
                break;
            case algebraic_type_1.BuiltinType.Type.I8:
                this.writer.writeI8(value);
                break;
            case algebraic_type_1.BuiltinType.Type.U8:
                this.writer.writeU8(value);
                break;
            case algebraic_type_1.BuiltinType.Type.I16:
                this.writer.writeI16(value);
                break;
            case algebraic_type_1.BuiltinType.Type.U16:
                this.writer.writeU16(value);
                break;
            case algebraic_type_1.BuiltinType.Type.I32:
                this.writer.writeI32(value);
                break;
            case algebraic_type_1.BuiltinType.Type.U32:
                this.writer.writeU32(value);
                break;
            case algebraic_type_1.BuiltinType.Type.I64:
                this.writer.writeI64(value);
                break;
            case algebraic_type_1.BuiltinType.Type.U64:
                this.writer.writeU64(value);
                break;
            case algebraic_type_1.BuiltinType.Type.I128:
                this.writer.writeI128(value);
                break;
            case algebraic_type_1.BuiltinType.Type.U128:
                this.writer.writeU128(value);
                break;
            case algebraic_type_1.BuiltinType.Type.F32:
                this.writer.writeF32(value);
                break;
            case algebraic_type_1.BuiltinType.Type.F64:
                this.writer.writeF64(value);
                break;
        }
    }
    writeByte(byte) {
        this.writer.writeU8(byte);
    }
}
exports.BinarySerializer = BinarySerializer;
