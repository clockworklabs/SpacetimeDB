using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace SpacetimeDB
{
    public static class Utils
    {
        public static bool ByteArrayCompare(byte[] a1, byte[] a2)
        {
            if (a1 == null || a2 == null)
                return a1 == a2;

            if (a1.Length != a2.Length)
                return false;

            for (int i = 0; i < a1.Length; i++)
                if (a1[i] != a2[i])
                    return false;

            return true;
        }
    }

    public class ByteArrayComparer : IEqualityComparer<byte[]>
    {
        public bool Equals(byte[] x, byte[] y)
        {
            return Utils.ByteArrayCompare(x, y);
        }

        public int GetHashCode(byte[] obj)
        {
            if (obj == null)
                return 0;
            int sum = 0;
            for (int i = 0; i < obj.Length; i++)
                sum += obj[i];
            return sum;
        }
    }
}
