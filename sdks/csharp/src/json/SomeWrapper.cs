using System.Collections;
using System.Collections.Generic;

namespace SpacetimeDB
{
    public class SomeWrapper<T>
    {
        public T Value { get; set; }

        public SomeWrapper(T value)
        {
            Value = value;
        }
    }
}
