using System.Collections;
using System.Collections.Generic;
using UnityEngine;

namespace SpacetimeDB
{
    public partial class Reducer
    {
    }

    public partial class ReducerCallInfo
    {
        public SpacetimeDB.NetworkManager.DbEvent[] RowChanges { get; internal set; }
    }
}