using System;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;

namespace SpacetimeDB
{
    public delegate void ProcedureCallback<T>(IProcedureEventContext ctx, ProcedureCallbackResult<T> result);

    public readonly struct ProcedureCallbackResult<T>
    {
        public readonly bool IsSuccess;
        public readonly T? Value;
        public readonly Exception? Error;

        public static ProcedureCallbackResult<T> Success(T value) => new(true, value, null);
        public static ProcedureCallbackResult<T> Failure(Exception error) => new(false, default, error);

        private ProcedureCallbackResult(bool isSuccess, T? value, Exception? error)
        {
            IsSuccess = isSuccess;
            Value = value;
            Error = error;
        }
    }

    internal sealed class ProcedureCallbacks
    {
        private readonly object callbacksLock = new();
        private readonly Dictionary<uint, IProcedureCallbackWrapper> callbacks = new();

        public void RegisterCallback<T>(uint requestId, ProcedureCallback<T> callback)
            where T : IStructuralReadWrite, new()
        {
            lock (callbacksLock)
            {
                callbacks[requestId] = new ProcedureCallbackWrapper<T>(callback);
            }
        }

        public bool TryResolveCallback(IProcedureEventContext ctx, uint requestId, ProcedureResult result)
        {
            IProcedureCallbackWrapper? wrapper;
            lock (callbacksLock)
            {
                if (!callbacks.Remove(requestId, out wrapper))
                {
                    return false;
                }
            }
            wrapper.Invoke(ctx, result);
            return true;
        }

        public void FailAll(IProcedureEventContext ctx, Exception error)
        {
            IProcedureCallbackWrapper[] wrappers;
            lock (callbacksLock)
            {
                wrappers = callbacks.Values.ToArray();
                callbacks.Clear();
            }

            foreach (var wrapper in wrappers)
            {
                wrapper.InvokeFailure(ctx, error);
            }
        }

        public void Clear()
        {
            lock (callbacksLock)
            {
                callbacks.Clear();
            }
        }
    }

    internal interface IProcedureCallbackWrapper
    {
        void Invoke(IProcedureEventContext ctx, ProcedureResult result);
        void InvokeFailure(IProcedureEventContext ctx, Exception error);
    }

    internal sealed class ProcedureCallbackWrapper<T> : IProcedureCallbackWrapper
        where T : IStructuralReadWrite, new()
    {
        private readonly ProcedureCallback<T> callback;

        public ProcedureCallbackWrapper(ProcedureCallback<T> callback)
        {
            this.callback = callback;
        }

        public void Invoke(IProcedureEventContext ctx, ProcedureResult result)
        {
            var callbackResult = result.Status switch
            {
                ProcedureStatus.Returned(var bytes) =>
                    ProcedureCallbackResult<T>.Success(BSATNHelpers.Decode<T>(bytes.ToArray())),
                ProcedureStatus.InternalError(var error) =>
                    ProcedureCallbackResult<T>.Failure(new Exception($"Procedure failed: {error}")),
                _ => ProcedureCallbackResult<T>.Failure(new Exception("Unknown procedure status"))
            };

            callback(ctx, callbackResult);
        }

        public void InvokeFailure(IProcedureEventContext ctx, Exception error)
        {
            callback(ctx, ProcedureCallbackResult<T>.Failure(error));
        }
    }
}
