using System;
using System.Collections.Generic;
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
        private readonly Dictionary<uint, IProcedureCallbackWrapper> callbacks = new();

        public void RegisterCallback<T>(uint requestId, ProcedureCallback<T> callback)
            where T : IStructuralReadWrite, new()
        {
            callbacks[requestId] = new ProcedureCallbackWrapper<T>(callback);
        }

        public bool TryResolveCallback(IProcedureEventContext ctx, uint requestId, ProcedureResult result)
        {
            if (callbacks.Remove(requestId, out var wrapper))
            {
                wrapper.Invoke(ctx, result);
                return true;
            }
            return false;
        }
    }

    internal interface IProcedureCallbackWrapper
    {
        void Invoke(IProcedureEventContext ctx, ProcedureResult result);
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
                ProcedureStatus.OutOfEnergy =>
                    ProcedureCallbackResult<T>.Failure(new Exception("Procedure execution aborted due to insufficient energy")),
                _ => ProcedureCallbackResult<T>.Failure(new Exception("Unknown procedure status"))
            };

            callback(ctx, callbackResult);
        }
    }
}