using System;

namespace SpacetimeDB
{
    public class ReducerEvent : Attribute
    {
        public string FunctionName { get; set; }
    }
}
