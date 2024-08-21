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

        public SumType()
        {
            variants = new List<SumTypeVariant>();
        }

        // TODO(jdetter): Perhaps not needed?
        public SumType NewUnnamed()
        {
            var s = new SumType
            {
                variants = variants.Select(a => new SumTypeVariant(null, a.algebraicType)).ToList()
            };
            return s;
        }
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

        public ProductType()
        {
            elements = new List<ProductTypeElement>();
        }
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

    public class BuiltinType
    {
        public enum Type
        {
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
            Map
        }

        public Type type;

        public AlgebraicType arrayType;
        public MapType mapType;
    }

    public class AlgebraicType
    {
        public enum Type
        {
            Sum,
            Product,
            Builtin,
            TypeRef,
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
        
        public BuiltinType builtin {
            get { return type == Type.Builtin ? (BuiltinType)type_ : null; }
            set {
                type_ = value;
                type = value == null ? Type.None : Type.Builtin;
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
                product = new ProductType
                {
                    elements = elements.ToList()
                }
            };
        }
        
        public static AlgebraicType CreateSumType(IEnumerable<SumTypeVariant> variants)
        {
            return new AlgebraicType
            {
                type = Type.Sum,
                sum = new SumType
                {
                    variants = variants.ToList(),
                }
            };
        }

        public static AlgebraicType CreateArrayType(AlgebraicType elementType)  {
            return new AlgebraicType
            {
                type = Type.Builtin,
                builtin = new BuiltinType
                {
                    type = BuiltinType.Type.Array,
                    arrayType = elementType
                }
            };
        }

        public static AlgebraicType CreatePrimitiveType(BuiltinType.Type type)  {
            return new AlgebraicType
            {
                type = Type.Builtin,
                builtin = new BuiltinType
                {
                    type = type,
                }
            };
        }

    }
}