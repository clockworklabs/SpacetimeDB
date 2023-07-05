using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace SpacetimeDB
{
    public interface ISpacetimeDBLogger
    {
        void Log(string message);
        void LogError(string message);
        void LogWarning(string message);
        void LogException(Exception e);
    }
}
