using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Numerics;
using System.Runtime.InteropServices;

namespace SpacetimeDB.SATS
{
    public class SumType
    {
        public List<SumTypeVariant> variants;

        public SumType() => variants = new List<SumTypeVariant>();

        // TODO(jdetter): Perhaps not needed?
        public SumType NewUnnamed() => new SumType
        {
            variants = variants.Select(a => new SumTypeVariant(null, a.algebraicType)).ToList()
        };
    }

    public struct SumTypeVariant
    {
        public string name;
        public AlgebraicType algebraicType;

        public SumTypeVariant(string name, AlgebraicType algebraicType)
        {
            this.name = name;
            this.algebraicType = algebraicType;
        }
    }

    public class ProductType
    {
        public List<ProductTypeElement> elements;

        public ProductType() => elements = new List<ProductTypeElement>();
    }

    public struct ProductTypeElement
    {
        public string name;
        public AlgebraicType algebraicType;

        public ProductTypeElement(string name, AlgebraicType algebraicType)
        {
            this.name = name;
            this.algebraicType = algebraicType;
        }

        public ProductTypeElement(AlgebraicType algebraicType)
        {
            name = null;
            this.algebraicType = algebraicType;
        }
    }

    public struct MapType
    {
        public AlgebraicType keyType;
        public AlgebraicType valueType;
    }

    public class AlgebraicType
    {
        public enum Type
        {
            TypeRef,
            Sum,
            Product,
            Bool,
            I8,
            U8,
            I16,
            U16,
            I32,
            U32,
            I64,
            U64,
            I128,
            U128,
            F32,
            F64,
            String,
            Array,
            Map,
            None,
        }

        public Type type;
        private object type_;

        public SumType sum {
            get { return type == Type.Sum ? (SumType)type_ : null; }
            set {
                type_ = value;
                type = value == null ? Type.None : Type.Sum;
            }
        }

        public ProductType product {
            get { return type == Type.Product ? (ProductType)type_ : null; }
            set {
                type_ = value;
                type = value == null ? Type.None : Type.Product;
            }
        }

        public AlgebraicType array {
            get { return type == Type.Array ? (AlgebraicType)type_ : null; }
            set {
                type_ = value;
                type = value == null ? Type.None : Type.Array;
            }
        }

        public MapType map {
            get { return type == Type.Map ? (MapType)type_ : null; }
            set {
                type_ = value;
                type = value == null ? Type.None : Type.Map;
            }
        }

        public int typeRef {
            get { return type == Type.TypeRef ? (int)type_ : -1; }
            set {
                type_ = value;
                type = value == -1 ? Type.None : Type.TypeRef;
            }
        }

        public static AlgebraicType CreateProductType(IEnumerable<ProductTypeElement> elements)
        {
            return new AlgebraicType
            {
                type = Type.Product,
                type_ = new ProductType {
                    elements = elements.ToList()
                }
            };
        }

        public static AlgebraicType CreateSumType(IEnumerable<SumTypeVariant> variants)
        {
            return new AlgebraicType
            {
                type = Type.Sum,
                type_ = new SumType
                {
                    variants = variants.ToList(),
                }
            };
        }

        public static AlgebraicType CreateArrayType(AlgebraicType elementType)  {
            return new AlgebraicType
            {
                type = Type.Array,
                type_ = elementType
            };
        }

        public static AlgebraicType CreateBytesType() => AlgebraicType.CreateArrayType(AlgebraicType.CreateU8Type());

        public static AlgebraicType CreateMapType(MapType type)  {
            return new AlgebraicType
            {
                type = Type.Map,
                type_ = type
            };
        }

        public static AlgebraicType CreateTypeRef(int idx) {
            return new AlgebraicType
            {
                type = Type.TypeRef,
                type_ = idx
            };
        }

        public static AlgebraicType CreateBoolType() => new AlgebraicType { type = Type.Bool };
        public static AlgebraicType CreateI8Type() => new AlgebraicType { type = Type.I8 };
        public static AlgebraicType CreateU8Type() => new AlgebraicType { type = Type.U8 };
        public static AlgebraicType CreateI16Type() => new AlgebraicType { type = Type.I16 };
        public static AlgebraicType CreateU16Type() => new AlgebraicType { type = Type.U16 };
        public static AlgebraicType CreateI32Type() => new AlgebraicType { type = Type.I32 };
        public static AlgebraicType CreateU32Type() => new AlgebraicType { type = Type.U32 };
        public static AlgebraicType CreateI64Type() => new AlgebraicType { type = Type.I64 };
        public static AlgebraicType CreateU64Type() => new AlgebraicType { type = Type.U64 };
        public static AlgebraicType CreateI128Type() => new AlgebraicType { type = Type.I128 };
        public static AlgebraicType CreateU128Type() => new AlgebraicType { type = Type.U128 };
        public static AlgebraicType CreateF32Type() => new AlgebraicType { type = Type.F32 };
        public static AlgebraicType CreateF64Type() => new AlgebraicType { type = Type.F64 };
        public static AlgebraicType CreateStringType() => new AlgebraicType { type = Type.String };
    }
}
