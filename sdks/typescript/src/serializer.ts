import { AlgebraicType } from "./algebraic_type";
import BinaryWriter from "./binary_writer";
import { Identity } from "./identity";
import { Address } from "./address";
import { AlgebraicValue, MapValue } from "./algebraic_value";

export interface Serializer {
  write(type: AlgebraicType, value: any): any;
  args(): any;
}

export class BinarySerializer {
  private writer: BinaryWriter;

  constructor() {
    this.writer = new BinaryWriter(1024);
  }

  args(): any {
    return this.getBuffer();
  }

  getBuffer(): Uint8Array {
    return this.writer.getBuffer();
  }

  write(type: AlgebraicType, value: any) {
    switch (type.type) {
      case AlgebraicType.Type.ProductType: {
        for (const element of type.product.elements) {
          this.write(element.algebraicType, value[element.name]);
        }
        break;
      }
      case AlgebraicType.Type.SumType:
        const sum = type.sum;
        if (
          sum.variants.length == 2 &&
          sum.variants[0].name === "some" &&
          sum.variants[1].name === "none"
        ) {
          if (value) {
            this.writeByte(0);
            this.write(sum.variants[0].algebraicType, value);
          } else {
            this.writeByte(1);
          }
        } else {
          const index = type.sum.variants.findIndex(
            (v) => v.name === value.tag
          );
          if (index < 0) {
            throw `Can't serialize a sum type, couldn't find ${value.tag} tag`;
          }

          this.writeByte(index);
          this.write(sum.variants[index].algebraicType, value.value);
        }
        break;
      case AlgebraicType.Type.ArrayType:
        const elemTy = type.array;
        const array = value as any[];
        this.writer.writeU32(array.length);
        for (const element of array) {
          this.write(elemTy, element);
        }
        break;
      case AlgebraicType.Type.MapType:
        const mapTy = type.map;
        const map = value as MapValue;
        this.writer.writeU32(map.size);
        for (const entry of map) {
          this.write(mapTy.keyType, entry[0]);
          this.write(mapTy.valueType, entry[1]);
        }
        break;
      case AlgebraicType.Type.String:
        this.writer.writeString(value);
        break;
      case AlgebraicType.Type.Bool:
        this.writer.writeBool(value);
        break;
      case AlgebraicType.Type.I8:
        this.writer.writeI8(value);
        break;
      case AlgebraicType.Type.U8:
        this.writer.writeU8(value);
        break;
      case AlgebraicType.Type.I16:
        this.writer.writeI16(value);
        break;
      case AlgebraicType.Type.U16:
        this.writer.writeU16(value);
        break;
      case AlgebraicType.Type.I32:
        this.writer.writeI32(value);
        break;
      case AlgebraicType.Type.U32:
        this.writer.writeU32(value);
        break;
      case AlgebraicType.Type.I64:
        this.writer.writeI64(value);
        break;
      case AlgebraicType.Type.U64:
        this.writer.writeU64(value);
        break;
      case AlgebraicType.Type.I128:
        this.writer.writeI128(value);
        break;
      case AlgebraicType.Type.U128:
        this.writer.writeU128(value);
        break;
      case AlgebraicType.Type.F32:
        this.writer.writeF32(value);
        break;
      case AlgebraicType.Type.F64:
        this.writer.writeF64(value);
        break;
      default:
        break;
    }
  }

  writeByte(byte: number) {
    this.writer.writeU8(byte);
  }
}
