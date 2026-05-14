namespace SpacetimeDB;

using System;
using System.Diagnostics.CodeAnalysis;

#pragma warning disable STDB_UNSTABLE
public abstract class HandlerContextBase
{
    public Random Rng => txState.Rng;
    public Timestamp Timestamp => txState.Timestamp;

    // NOTE: The host rejects procedure HTTP requests while a mut transaction is open
    // (WOULD_BLOCK_TRANSACTION). Avoid calling `Http.*` inside WithTx.
    public HttpClient Http { get; } = new();

    // **Note:** must be 0..=u32::MAX
    protected int CounterUuid = 0;
    private readonly TransactionalContextState<HandlerTxContextBase> txState;

    protected HandlerContextBase(Random random, Timestamp time)
    {
        txState = new(
            random,
            time,
            timestamp =>
                new Internal.TxContext(
                    CreateLocal(),
                    default,
                    null,
                    timestamp,
                    AuthCtx.BuildFromSystemTables(null, default),
                    random
                ),
            inner => CreateTxContext(inner)
        );
    }

    protected abstract HandlerTxContextBase CreateTxContext(Internal.TxContext inner);
    protected internal abstract LocalBase CreateLocal();

    public Internal.TxContext EnterTxContext(long timestampMicros) => txState.EnterTxContext(timestampMicros);

    public void ExitTxContext() => txState.ExitTxContext();

    public readonly struct TxOutcome<TResult>(bool isSuccess, TResult? value, Exception? error)
    {
        public bool IsSuccess { get; } = isSuccess;
        public TResult? Value { get; } = value;
        public Exception? Error { get; } = error;

        public static TxOutcome<TResult> Success(TResult value) => new(true, value, null);

        public static TxOutcome<TResult> Failure(Exception error) => new(false, default, error);

        public TResult UnwrapOrThrow() =>
            IsSuccess
                ? Value!
                : throw (
                    Error
                    ?? new InvalidOperationException("Transaction failed without an error object.")
                );
    }

    [Experimental("STDB_UNSTABLE")]
    public TResult WithTx<TResult>(Func<HandlerTxContextBase, TResult> body) =>
        txState.WithTx(body);

    [Experimental("STDB_UNSTABLE")]
    public TxOutcome<TResult> TryWithTx<TResult, TError>(
        Func<HandlerTxContextBase, Result<TResult, TError>> body
    )
        where TError : Exception
    {
        var outcome = txState.TryWithTx(body);
        return outcome.IsSuccess
            ? TxOutcome<TResult>.Success(outcome.Value!)
            : TxOutcome<TResult>.Failure(
                outcome.Error
                ?? new InvalidOperationException("Transaction failed without an error object.")
            );
    }
}

public abstract class HandlerTxContextBase(Internal.TxContext inner) : IRefreshableTxContext
{
    internal Internal.TxContext Inner { get; private set; } = inner;

    internal void Refresh(Internal.TxContext inner) => Inner = inner;
    void IRefreshableTxContext.Refresh(Internal.TxContext inner) => Refresh(inner);

    public LocalBase Db => (LocalBase)Inner.Db;
    public Timestamp Timestamp => Inner.Timestamp;
    public Random Rng => Inner.Rng;
}

internal sealed partial class RuntimeHandlerContext(Random random, Timestamp timestamp)
    : HandlerContextBase(random, timestamp)
{
    private readonly RuntimeLocal _db = new();

    protected internal override LocalBase CreateLocal() => _db;

    protected override HandlerTxContextBase CreateTxContext(Internal.TxContext inner) =>
        _cached ??= new RuntimeHandlerTxContext(inner);

    private RuntimeHandlerTxContext? _cached;
}

internal sealed class RuntimeHandlerTxContext : HandlerTxContextBase
{
    internal RuntimeHandlerTxContext(Internal.TxContext inner)
        : base(inner) { }

    public new RuntimeLocal Db => (RuntimeLocal)base.Db;
}
#pragma warning restore STDB_UNSTABLE
