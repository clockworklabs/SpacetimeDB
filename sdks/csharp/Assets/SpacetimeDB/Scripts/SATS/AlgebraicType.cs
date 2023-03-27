using System.Collections.Generic;
using System.Linq;

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

        public static AlgebraicType MakeMetaType()
        {
            var string_type = AlgebraicType.CreatePrimitiveType(BuiltinType.Type.String);
            var option_type = AlgebraicType.CreateOptionType(string_type);
            var variant_type = AlgebraicType.CreateProductType(
                new ProductTypeElement("name", option_type),
                new ProductTypeElement("algebraic_type", AlgebraicType.CreateTypeRef(0))
            );
            var array = AlgebraicType.CreateArrayType(variant_type);
            return AlgebraicType.CreateProductType(new ProductTypeElement("variants", array));
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

        public static AlgebraicType MakeMetaType()
        {
            var string_type = AlgebraicType.CreatePrimitiveType(BuiltinType.Type.String);
            var option_type = AlgebraicType.CreateOptionType(string_type);
            var element_type = AlgebraicType.CreateProductType(
                new ProductTypeElement("name", option_type),
                new ProductTypeElement("algebraic_type", AlgebraicType.CreateTypeRef(0))
            );
            var array = AlgebraicType.CreateArrayType(element_type);
            return AlgebraicType.CreateProductType(new ProductTypeElement("elements", array));
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

        public static AlgebraicType MakeMetaType()
        {
            return AlgebraicType.CreateSumType(
                new SumTypeVariant("bool", AlgebraicType.CreateProductType()),
                new SumTypeVariant("i8", AlgebraicType.CreateProductType()),
                new SumTypeVariant("u8", AlgebraicType.CreateProductType()),
                new SumTypeVariant("i16", AlgebraicType.CreateProductType()),
                new SumTypeVariant("u16", AlgebraicType.CreateProductType()),
                new SumTypeVariant("i32", AlgebraicType.CreateProductType()),
                new SumTypeVariant("u32", AlgebraicType.CreateProductType()),
                new SumTypeVariant("i64", AlgebraicType.CreateProductType()),
                new SumTypeVariant("u64", AlgebraicType.CreateProductType()),
                new SumTypeVariant("i128", AlgebraicType.CreateProductType()),
                new SumTypeVariant("u128", AlgebraicType.CreateProductType()),
                new SumTypeVariant("f32", AlgebraicType.CreateProductType()),
                new SumTypeVariant("f64", AlgebraicType.CreateProductType()),
                new SumTypeVariant("string", AlgebraicType.CreateProductType()),
                new SumTypeVariant("array", AlgebraicType.CreateTypeRef(0)),
                new SumTypeVariant(
                    "map",
                    AlgebraicType.CreateProductType(
                        new ProductTypeElement("key_ty", AlgebraicType.CreateTypeRef(0)),
                        new ProductTypeElement("ty", AlgebraicType.CreateTypeRef(0))
                    )
                )
            );
        }
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

        public SumType sum
        {
            get { return type == Type.Sum ? (SumType)type_ : null; }
            set
            {
                type_ = value;
                type = value == null ? Type.None : Type.Sum;
            }
        }

        public ProductType product
        {
            get { return type == Type.Product ? (ProductType)type_ : null; }
            set
            {
                type_ = value;
                type = value == null ? Type.None : Type.Product;
            }
        }

        public BuiltinType builtin
        {
            get { return type == Type.Builtin ? (BuiltinType)type_ : null; }
            set
            {
                type_ = value;
                type = value == null ? Type.None : Type.Builtin;
            }
        }

        public int typeRef
        {
            get { return type == Type.TypeRef ? (int)type_ : -1; }
            set
            {
                type_ = value;
                type = value == -1 ? Type.None : Type.TypeRef;
            }
        }

        public static AlgebraicType CreateProductType(params ProductTypeElement[] elements)
        {
            return new AlgebraicType
            {
                type = Type.Product,
                product = new ProductType { elements = elements.ToList() }
            };
        }

        public static AlgebraicType CreateProductType(IEnumerable<ProductTypeElement> elements)
        {
            return CreateProductType(elements.ToArray());
        }

        public static AlgebraicType CreateSumType(params SumTypeVariant[] variants)
        {
            return new AlgebraicType
            {
                type = Type.Sum,
                sum = new SumType { variants = variants.ToList() }
            };
        }

        public static AlgebraicType CreateSumType(IEnumerable<SumTypeVariant> variants)
        {
            return CreateSumType(variants.ToArray());
        }

        public static AlgebraicType CreateArrayType(AlgebraicType elementType)
        {
            return new AlgebraicType
            {
                type = Type.Builtin,
                builtin = new BuiltinType { type = BuiltinType.Type.Array, arrayType = elementType }
            };
        }

        public static AlgebraicType CreatePrimitiveType(BuiltinType.Type type)
        {
            return new AlgebraicType
            {
                type = Type.Builtin,
                builtin = new BuiltinType { type = type }
            };
        }

        public static AlgebraicType CreateTypeRef(int typeRef)
        {
            return new AlgebraicType { type = Type.TypeRef, typeRef = typeRef };
        }

        public static AlgebraicType CreateOptionType(AlgebraicType elementType)
        {
            return CreateSumType(
                new SumTypeVariant("some", CreateProductType(new ProductTypeElement(elementType))),
                new SumTypeVariant("none", CreateProductType())
            );
        }

        public static AlgebraicType MakeMetaType()
        {
            return AlgebraicType.CreateSumType(
                new SumTypeVariant("sum", SumType.MakeMetaType()),
                new SumTypeVariant("product", ProductType.MakeMetaType()),
                new SumTypeVariant("builtin", BuiltinType.MakeMetaType()),
                new SumTypeVariant("ref", AlgebraicType.CreatePrimitiveType(BuiltinType.Type.U32))
            );
        }
    }
}
