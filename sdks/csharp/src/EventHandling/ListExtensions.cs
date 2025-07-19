using System;
using System.Collections.Generic;

namespace SpacetimeDB.EventHandling
{
    internal static class ListExtensions
    {
        public static void RemoveAtSwapBack<T>(this List<T> list, int index)
        {
            if (list == null) throw new ArgumentNullException(nameof(list));

            var lastIndex = list.Count - 1;

            if (index < 0 || index > lastIndex) throw new ArgumentOutOfRangeException(nameof(index), "Index is out of range.");

            if (index == lastIndex)
            {
                list.RemoveAt(index);
                return;
            }

            list[index] = list[lastIndex];

            list.RemoveAt(lastIndex);
        }
    }
}