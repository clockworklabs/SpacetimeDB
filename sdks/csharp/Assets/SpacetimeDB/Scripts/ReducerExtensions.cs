using System;

namespace SpacetimeDB
{
    public class ReducerEvent : Attribute
    {
        public string FunctionName { get; set; }
    }

    public class DeserializeEvent : Attribute
    {
        public string FunctionName { get; set; }
    }
}
