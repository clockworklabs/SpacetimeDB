using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace SpacetimeDB
{
    class ReducerMismatchException : Exception
    {
        public ReducerMismatchException(string originalReducerName, string attemptedConversionReducerName) 
            : base($"Cannot cast agruments from {originalReducerName} reducer call into {attemptedConversionReducerName} reducer arguments")
        {
        }
    }
}
