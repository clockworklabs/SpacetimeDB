#ifndef SPACETIMEDB_INTERNAL_TX_EXECUTION_H
#define SPACETIMEDB_INTERNAL_TX_EXECUTION_H

#include <spacetimedb/abi/FFI.h>
#include <spacetimedb/outcome.h>
#include <spacetimedb/tx_context.h>
#include <type_traits>
#include <utility>

namespace SpacetimeDB::Internal {

#ifdef SPACETIMEDB_UNSTABLE_FEATURES

template<typename T>
struct is_outcome : std::false_type {};

template<typename T>
struct is_outcome<Outcome<T>> : std::true_type {};

template<typename T>
inline constexpr bool is_outcome_v = is_outcome<std::remove_cv_t<std::remove_reference_t<T>>>::value;

template<typename T>
bool tx_result_should_commit(const T& result) {
    using ResultType = std::remove_cv_t<std::remove_reference_t<T>>;
    // TODO(http-handlers-cpp): Consider tightening try_with_tx in a future breaking release
    // so rollback-aware callbacks use Outcome<T> (and possibly bool for compatibility)
    // instead of silently treating arbitrary return types as commit-on-success.
    if constexpr (std::is_same_v<ResultType, bool>) {
        return result;
    } else if constexpr (is_outcome_v<ResultType>) {
        return result.is_ok();
    } else {
        return true;
    }
}

class TxAbortGuard {
public:
    TxAbortGuard() = default;
    TxAbortGuard(const TxAbortGuard&) = delete;
    TxAbortGuard& operator=(const TxAbortGuard&) = delete;

    ~TxAbortGuard() {
        if (!armed_) {
            return;
        }
        Status status = FFI::procedure_abort_mut_tx();
        if (is_error(status)) {
            LOG_PANIC("Failed to abort transaction");
        }
    }

    void disarm() {
        armed_ = false;
    }

private:
    bool armed_ = true;
};

inline void commit_tx_or_panic() {
    Status status = FFI::procedure_commit_mut_tx();
    if (is_error(status)) {
        LOG_PANIC("Failed to commit transaction");
    }
}

inline bool try_commit_tx() {
    return is_ok(FFI::procedure_commit_mut_tx());
}

inline void abort_tx_or_panic() {
    Status status = FFI::procedure_abort_mut_tx();
    if (is_error(status)) {
        LOG_PANIC("Failed to abort transaction");
    }
}

template<typename MakeReducerContext, typename Func>
auto run_tx_once(MakeReducerContext&& make_reducer_ctx, Func& body) -> decltype(body(std::declval<TxContext&>())) {
    using ResultType = decltype(body(std::declval<TxContext&>()));

    int64_t tx_timestamp = 0;
    Status status = FFI::procedure_start_mut_tx(&tx_timestamp);
    if (is_error(status)) {
        LOG_PANIC("Failed to start transaction");
    }

    TxAbortGuard abort_guard;
    ReducerContext reducer_ctx = make_reducer_ctx(Timestamp::from_micros_since_epoch(tx_timestamp));
    TxContext tx{reducer_ctx};

    if constexpr (std::is_void_v<ResultType>) {
        body(tx);
        abort_guard.disarm();
    } else {
        ResultType result = body(tx);
        abort_guard.disarm();
        return result;
    }
}

template<typename MakeReducerContext, typename Func>
auto with_tx(MakeReducerContext&& make_reducer_ctx, Func& body) -> decltype(body(std::declval<TxContext&>())) {
    using ResultType = decltype(body(std::declval<TxContext&>()));

    if constexpr (std::is_void_v<ResultType>) {
        run_tx_once(std::forward<MakeReducerContext>(make_reducer_ctx), body);
        if (!try_commit_tx()) {
            run_tx_once(std::forward<MakeReducerContext>(make_reducer_ctx), body);
            commit_tx_or_panic();
        }
    } else {
        ResultType result = run_tx_once(std::forward<MakeReducerContext>(make_reducer_ctx), body);
        if (!try_commit_tx()) {
            result = run_tx_once(std::forward<MakeReducerContext>(make_reducer_ctx), body);
            commit_tx_or_panic();
        }
        return result;
    }
}

template<typename MakeReducerContext, typename Func>
auto try_with_tx(MakeReducerContext&& make_reducer_ctx, Func& body) -> decltype(body(std::declval<TxContext&>())) {
    using ResultType = decltype(body(std::declval<TxContext&>()));

    ResultType result = run_tx_once(std::forward<MakeReducerContext>(make_reducer_ctx), body);
    if (!tx_result_should_commit(result)) {
        abort_tx_or_panic();
        return result;
    }

    if (!try_commit_tx()) {
        result = run_tx_once(std::forward<MakeReducerContext>(make_reducer_ctx), body);
        if (tx_result_should_commit(result)) {
            commit_tx_or_panic();
        } else {
            abort_tx_or_panic();
        }
    }

    return result;
}

#endif

} // namespace SpacetimeDB::Internal

#endif // SPACETIMEDB_INTERNAL_TX_EXECUTION_H
