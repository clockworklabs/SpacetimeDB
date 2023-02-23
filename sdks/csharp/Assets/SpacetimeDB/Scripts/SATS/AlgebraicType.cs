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

    [StructLayout(LayoutKind.Explicit)]
    public struct BuiltinType
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

        [FieldOffset(0)] public Type type;
        [FieldOffset(4)] public AlgebraicType arrayType;
        [FieldOffset(4)] public MapType mapType;
    }

    [StructLayout(LayoutKind.Explicit)]
    public class AlgebraicType
    {
        public enum Type
        {
            Sum,
            Product,
            Builtin,
            TypeRef
        }

        [FieldOffset(0)] public Type type;
        [FieldOffset(4)] public SumType sum;
        [FieldOffset(4)] public ProductType product;
        [FieldOffset(4)] public BuiltinType builtin;
        [FieldOffset(4)] public int typeRef;

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
    }
}