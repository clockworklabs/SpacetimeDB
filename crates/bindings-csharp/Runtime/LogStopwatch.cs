using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace SpacetimeDB
{
    public sealed class LogStopwatch : IDisposable
    {
        private uint StopwatchId;

        public LogStopwatch(string name)
        {
            StopwatchId = StartStopwatchInternal(name);
        }

        private static uint StartStopwatchInternal(string name)
        {
            var name_bytes = Encoding.UTF8.GetBytes(name);
            var id = Internal.FFI._console_timer_start(name_bytes, (uint)name_bytes.Length);
            return id;
        }

        void IDisposable.Dispose()
        {
            End();
        }

        public void End()
        {
            Internal.FFI._console_timer_end(StopwatchId);
        }
    }
}
