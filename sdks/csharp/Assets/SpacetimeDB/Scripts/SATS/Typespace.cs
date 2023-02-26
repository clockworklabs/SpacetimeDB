using System.Collections;
using System.Collections.Generic;
using UnityEditor;
using UnityEngine;

namespace SpacetimeDB.SATS
{
    public class Typespace
    {
        public int rootRef;
        public List<AlgebraicType> types;

        public Typespace()
        {
            rootRef = 0;
            types = new List<AlgebraicType>();
        }

        public void Add(AlgebraicType type)
        {
            types.Add(type);
        }

        public AlgebraicType GetRoot() => types[rootRef];
    }

    public struct GenCtx
    {
        public Typespace typespace;
        public List<string> names;
    }
}