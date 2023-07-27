using System;
using System.Collections;
using System.Collections.Generic;

namespace SpacetimeDB
{
    [AttributeUsage(AttributeTargets.Property | AttributeTargets.Field)]
    public class SomeAttribute : Attribute 
    {
    }
}
