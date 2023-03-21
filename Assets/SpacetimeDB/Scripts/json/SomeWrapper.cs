using System.Collections;
using System.Collections.Generic;
using UnityEngine;

namespace Namespace 
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
