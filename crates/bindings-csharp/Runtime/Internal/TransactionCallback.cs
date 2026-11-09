#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
namespace SpacetimeDB.Internal;

using System.Reflection;
using System.Runtime.CompilerServices;

internal static class TransactionCallback
{
    internal static void Validate(Delegate body, Type returnType)
    {
        ArgumentNullException.ThrowIfNull(body);
        if (
            body.GetInvocationList().Length != 1
            || body.Method.IsDefined(typeof(AsyncStateMachineAttribute))
            || IsAwaitable(returnType)
            || IsAwaitable(body.Method.ReturnType)
        )
        {
            throw new ArgumentException(
                "Transactions require a single synchronous callback; async void, Task and ValueTask are not supported.",
                nameof(body)
            );
        }
    }

    internal static void ValidateResult<T>(T result)
    {
        if (result is not null && IsAwaitable(result.GetType()))
        {
            throw new ArgumentException("Transaction callbacks must not return Task or ValueTask.");
        }
    }

    private static bool IsAwaitable(Type type) =>
        typeof(Task).IsAssignableFrom(type)
        || type == typeof(ValueTask)
        || (type.IsGenericType && type.GetGenericTypeDefinition() == typeof(ValueTask<>));
}
#endif
