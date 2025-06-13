using System.Diagnostics;
using System.Diagnostics.CodeAnalysis;
using System.Text;
using CsCheck;

public partial class SmallHashSetTests
{
    Gen<List<(int Value, bool Remove)>> GenOperationList = Gen.Int[0, 32].SelectMany(count =>
        Gen.Select(Gen.Int[0, 3].List[count], Gen.Bool.List[count], (values, removes) => values.Zip(removes).ToList())
    );

    class IntEqualityComparer : IEqualityComparer<int>
    {
        public bool Equals(int x, int y)
            => x == y;

        public int GetHashCode([DisallowNull] int obj)
            => obj.GetHashCode();
    }

    [Fact]
    public void SmallHashSetIsLikeHashSet()
    {
        GenOperationList.Sample(ops =>
        {
            HashSet<int> ints = new(new IntEqualityComparer());
            SmallHashSet<int, IntEqualityComparer> smallInts = new();
            foreach (var it in ops)
            {
                var (value, remove) = it;
                if (remove)
                {
                    ints.Remove(value);
                    smallInts.Remove(value);
                }
                else
                {
                    ints.Add(value);
                    smallInts.Add(value);
                }
                Debug.Assert(ints.SetEquals(smallInts), $"{CollectionToString(ints)} != {CollectionToString(smallInts)}");
            }

        }, iter: 10_000);

    }

    [SpacetimeDB.Type]
    partial class IntHolder
    {
        public int Int;
    }

    [Fact]
    public void SmallHashSet_PreHashedRow_IsLikeHashSet()
    {
        GenOperationList.Sample(ops =>
        {
            HashSet<PreHashedRow> ints = new(new PreHashedRowComparer());
            SmallHashSetOfPreHashedRow smallInts = new();
            foreach (var it in ops)
            {
                var (valueWrapped, remove) = it;
                var value = new PreHashedRow(new IntHolder { Int = valueWrapped });

                if (remove)
                {
                    ints.Remove(value);
                    smallInts.Remove(value);
                }
                else
                {
                    ints.Add(value);
                    smallInts.Add(value);
                }
                Debug.Assert(ints.SetEquals(smallInts), $"{CollectionToString(ints)} != {CollectionToString(smallInts)}");
            }

        }, iter: 10_000);

    }

    string CollectionToString<T>(IEnumerable<T> collection)
    {
        StringBuilder result = new();
        result.Append("{");
        bool first = true;
        foreach (var item in collection)
        {
            if (!first)
            {
                result.Append($", ");
            }
            first = false;
            result.Append(item);
        }
        result.Append("}");
        return result.ToString();
    }

}