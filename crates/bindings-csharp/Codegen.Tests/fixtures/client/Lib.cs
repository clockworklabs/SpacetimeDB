﻿using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using SpacetimeDB;

#pragma warning disable CA1050 // Declare types in namespaces - this is a test fixture, no need for a namespace.

[SpacetimeDB.Type]
public partial struct CustomStruct
{
    public const int IGNORE_ME = 0;
    public static readonly string IGNORE_ME_TOO = "";
    public int IntField;
    public string StringField;
}

[SpacetimeDB.Type]
public partial struct CustomClass
{
    public const int IGNORE_ME = 0;
    public static readonly string IGNORE_ME_TOO = "";
    public int IntField;
    public string StringField;
}

[StructLayout(LayoutKind.Auto)]
public partial struct CustomClass
{
    public int IgnoreExtraFields;
}

[SpacetimeDB.Type]
public enum CustomEnum
{
    EnumVariant1,
    EnumVariant2,
}

namespace System.Runtime.CompilerServices
{
    internal static class IsExternalInit { } // https://stackoverflow.com/a/64749403/1484415
}

[SpacetimeDB.Type]
public partial record CustomTaggedEnum
    : SpacetimeDB.TaggedEnum<(int IntVariant, string StringVariant)>;

[SpacetimeDB.Type]
public partial struct PublicTable
{
    public byte ByteField;
    public ushort UshortField;
    public uint UintField;
    public ulong UlongField;
    public U128 U128Field;
    public U256 U256Field;
    public sbyte SbyteField;
    public short ShortField;
    public int IntField;
    public long LongField;
    public I128 I128Field;
    public I256 I256Field;
    public bool BoolField;
    public float FloatField;
    public double DoubleField;
    public string StringField;
    public Identity IdentityField;
    public ConnectionId ConnectionIdField;
    public CustomStruct CustomStructField;
    public CustomClass CustomClassField;
    public CustomEnum CustomEnumField;
    public CustomTaggedEnum CustomTaggedEnumField;
    public List<int> ListField;
    public int? NullableValueField;
    public string? NullableReferenceField;
}
