namespace SpacetimeDB.Internal;

using SpacetimeDB.BSATN;

enum BoundVariant : byte
{
    Inclusive,
    Exclusive,
    Unbounded,
}

public interface IIndexScanRangeBounds
{
    ushort PrefixElems { get; }
    void Prefix(BinaryWriter w);
    void RStart(BinaryWriter w);
    void REnd(BinaryWriter w);
}

public readonly struct Bound<T>(T min, T max)
    where T : IEquatable<T>
{
    public T Min => min;
    public T Max => max;

    public static implicit operator Bound<T>(T value) => new(value, value);

    public static implicit operator Bound<T>((T min, T max) span) => new(span.min, span.max);
}

public readonly struct IndexScanRangeBounds<T, TRW>(Bound<T> t) : IIndexScanRangeBounds
    where T : IEquatable<T>
    where TRW : struct, IReadWrite<T>
{
    public ushort PrefixElems => 0;

    public void Prefix(BinaryWriter _) { }

    public void RStart(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new TRW().Write(w, t.Min);
    }

    public void REnd(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new TRW().Write(w, t.Max);
    }
}

public readonly struct IndexScanRangeBounds<T, TRW, U, URW>((T t, Bound<U> u) b) : IIndexScanRangeBounds
    where U : IEquatable<U>
    where TRW : struct, IReadWrite<T>
    where URW : struct, IReadWrite<U>
{
    public ushort PrefixElems => 1;

    public void Prefix(BinaryWriter w)
    {
        new TRW().Write(w, b.t);
    }

    public void RStart(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new URW().Write(w, b.u.Min);
    }

    public void REnd(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new URW().Write(w, b.u.Max);
    }
}

public readonly struct IndexScanRangeBounds<T, TRW, U, URW, V, VRW>((T t, U u, Bound<V> v) b)
    : IIndexScanRangeBounds
    where V : IEquatable<V>
    where TRW : struct, IReadWrite<T>
    where URW : struct, IReadWrite<U>
    where VRW : struct, IReadWrite<V>
{
    public ushort PrefixElems => 2;

    public void Prefix(BinaryWriter w)
    {
        new TRW().Write(w, b.t);
        new URW().Write(w, b.u);
    }

    public void RStart(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new VRW().Write(w, b.v.Min);
    }

    public void REnd(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new VRW().Write(w, b.v.Max);
    }
}

public readonly struct IndexScanRangeBounds<T, TRW, U, URW, V, VRW, W, WRW>(
    (T t, U u, V v, Bound<W> w) b
) : IIndexScanRangeBounds
    where W : IEquatable<W>
    where TRW : struct, IReadWrite<T>
    where URW : struct, IReadWrite<U>
    where VRW : struct, IReadWrite<V>
    where WRW : struct, IReadWrite<W>
{
    public ushort PrefixElems => 3;

    public void Prefix(BinaryWriter w)
    {
        new TRW().Write(w, b.t);
        new URW().Write(w, b.u);
        new VRW().Write(w, b.v);
    }

    public void RStart(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new WRW().Write(w, b.w.Min);
    }

    public void REnd(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new WRW().Write(w, b.w.Max);
    }
}

public readonly struct IndexScanRangeBounds<T, TRW, U, URW, V, VRW, W, WRW, X, XRW>(
    (T t, U u, V v, W w, Bound<X> x) b
) : IIndexScanRangeBounds
    where X : IEquatable<X>
    where TRW : struct, IReadWrite<T>
    where URW : struct, IReadWrite<U>
    where VRW : struct, IReadWrite<V>
    where WRW : struct, IReadWrite<W>
    where XRW : struct, IReadWrite<X>
{
    public ushort PrefixElems => 4;

    public void Prefix(BinaryWriter w)
    {
        new TRW().Write(w, b.t);
        new URW().Write(w, b.u);
        new VRW().Write(w, b.v);
        new WRW().Write(w, b.w);
    }

    public void RStart(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new XRW().Write(w, b.x.Min);
    }

    public void REnd(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new XRW().Write(w, b.x.Max);
    }
}

public readonly struct IndexScanRangeBounds<T, TRW, U, URW, V, VRW, W, WRW, X, XRW, Y, YRW>(
    (T t, U u, V v, W w, X x, Bound<Y> y) b
) : IIndexScanRangeBounds
    where Y : IEquatable<Y>
    where TRW : struct, IReadWrite<T>
    where URW : struct, IReadWrite<U>
    where VRW : struct, IReadWrite<V>
    where WRW : struct, IReadWrite<W>
    where XRW : struct, IReadWrite<X>
    where YRW : struct, IReadWrite<Y>
{
    public ushort PrefixElems => 5;

    public void Prefix(BinaryWriter w)
    {
        new TRW().Write(w, b.t);
        new URW().Write(w, b.u);
        new VRW().Write(w, b.v);
        new WRW().Write(w, b.w);
        new XRW().Write(w, b.x);
    }

    public void RStart(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new YRW().Write(w, b.y.Min);
    }

    public void REnd(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new YRW().Write(w, b.y.Max);
    }
}

public readonly struct IndexScanRangeBounds<T, TRW, U, URW, V, VRW, W, WRW, X, XRW, Y, YRW, Z, ZRW>(
    (T t, U u, V v, W w, X x, Y y, Bound<Z> z) b
) : IIndexScanRangeBounds
    where Z : IEquatable<Z>
    where TRW : struct, IReadWrite<T>
    where URW : struct, IReadWrite<U>
    where VRW : struct, IReadWrite<V>
    where WRW : struct, IReadWrite<W>
    where XRW : struct, IReadWrite<X>
    where YRW : struct, IReadWrite<Y>
    where ZRW : struct, IReadWrite<Z>
{
    public ushort PrefixElems => 6;

    public void Prefix(BinaryWriter w)
    {
        new TRW().Write(w, b.t);
        new URW().Write(w, b.u);
        new VRW().Write(w, b.v);
        new WRW().Write(w, b.w);
        new XRW().Write(w, b.x);
        new YRW().Write(w, b.y);
    }

    public void RStart(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new ZRW().Write(w, b.z.Min);
    }

    public void REnd(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new ZRW().Write(w, b.z.Max);
    }
}

public readonly struct IndexScanRangeBounds<
    T,
    TRW,
    U,
    URW,
    V,
    VRW,
    W,
    WRW,
    X,
    XRW,
    Y,
    YRW,
    Z,
    ZRW,
    A,
    ARW
>((T t, U u, V v, W w, X x, Y y, Z z, Bound<A> a) b) : IIndexScanRangeBounds
    where A : IEquatable<A>
    where TRW : struct, IReadWrite<T>
    where URW : struct, IReadWrite<U>
    where VRW : struct, IReadWrite<V>
    where WRW : struct, IReadWrite<W>
    where XRW : struct, IReadWrite<X>
    where YRW : struct, IReadWrite<Y>
    where ZRW : struct, IReadWrite<Z>
    where ARW : struct, IReadWrite<A>
{
    public ushort PrefixElems => 7;

    public void Prefix(BinaryWriter w)
    {
        new TRW().Write(w, b.t);
        new URW().Write(w, b.u);
        new VRW().Write(w, b.v);
        new WRW().Write(w, b.w);
        new XRW().Write(w, b.x);
        new YRW().Write(w, b.y);
        new ZRW().Write(w, b.z);
    }

    public void RStart(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new ARW().Write(w, b.a.Min);
    }

    public void REnd(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new ARW().Write(w, b.a.Max);
    }
}

public readonly struct IndexScanRangeBounds<
    T,
    TRW,
    U,
    URW,
    V,
    VRW,
    W,
    WRW,
    X,
    XRW,
    Y,
    YRW,
    Z,
    ZRW,
    A,
    ARW,
    B,
    BRW
>((T t, U u, V v, W w, X x, Y y, Z z, A a, Bound<B> b) b) : IIndexScanRangeBounds
    where B : IEquatable<B>
    where TRW : struct, IReadWrite<T>
    where URW : struct, IReadWrite<U>
    where VRW : struct, IReadWrite<V>
    where WRW : struct, IReadWrite<W>
    where XRW : struct, IReadWrite<X>
    where YRW : struct, IReadWrite<Y>
    where ZRW : struct, IReadWrite<Z>
    where ARW : struct, IReadWrite<A>
    where BRW : struct, IReadWrite<B>
{
    public ushort PrefixElems => 8;

    public void Prefix(BinaryWriter w)
    {
        new TRW().Write(w, b.t);
        new URW().Write(w, b.u);
        new VRW().Write(w, b.v);
        new WRW().Write(w, b.w);
        new XRW().Write(w, b.x);
        new YRW().Write(w, b.y);
        new ZRW().Write(w, b.z);
        new ARW().Write(w, b.a);
    }

    public void RStart(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new BRW().Write(w, b.b.Min);
    }

    public void REnd(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new BRW().Write(w, b.b.Max);
    }
}

public readonly struct IndexScanRangeBounds<
    T,
    TRW,
    U,
    URW,
    V,
    VRW,
    W,
    WRW,
    X,
    XRW,
    Y,
    YRW,
    Z,
    ZRW,
    A,
    ARW,
    B,
    BRW,
    C,
    CRW
>((T t, U u, V v, W w, X x, Y y, Z z, A a, B b, Bound<C> c) b) : IIndexScanRangeBounds
    where C : IEquatable<C>
    where TRW : struct, IReadWrite<T>
    where URW : struct, IReadWrite<U>
    where VRW : struct, IReadWrite<V>
    where WRW : struct, IReadWrite<W>
    where XRW : struct, IReadWrite<X>
    where YRW : struct, IReadWrite<Y>
    where ZRW : struct, IReadWrite<Z>
    where ARW : struct, IReadWrite<A>
    where BRW : struct, IReadWrite<B>
    where CRW : struct, IReadWrite<C>
{
    public ushort PrefixElems => 9;

    public void Prefix(BinaryWriter w)
    {
        new TRW().Write(w, b.t);
        new URW().Write(w, b.u);
        new VRW().Write(w, b.v);
        new WRW().Write(w, b.w);
        new XRW().Write(w, b.x);
        new YRW().Write(w, b.y);
        new ZRW().Write(w, b.z);
        new ARW().Write(w, b.a);
        new BRW().Write(w, b.b);
    }

    public void RStart(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new CRW().Write(w, b.c.Min);
    }

    public void REnd(BinaryWriter w)
    {
        w.Write((byte)BoundVariant.Inclusive);
        new CRW().Write(w, b.c.Max);
    }
}
