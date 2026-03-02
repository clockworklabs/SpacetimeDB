#ifndef SPACETIMEDB_TEMPLATE_UTILS_H
#define SPACETIMEDB_TEMPLATE_UTILS_H

#include <cstddef>
#include <optional>
#include <tuple>
#include <utility>
#include <vector>

namespace SpacetimeDB {
namespace Internal {

template<typename T>
struct function_traits;

template<typename R, typename... Args>
struct function_traits<R(*)(Args...)> {
    static constexpr size_t arity = sizeof...(Args);
    using result_type = R;

    template<size_t N>
    using arg_t = typename std::tuple_element<N, std::tuple<Args...>>::type;
};

template<typename T>
std::vector<T> view_result_to_vec(std::vector<T>&& vec) {
    return std::move(vec);
}

template<typename T>
std::vector<T> view_result_to_vec(const std::vector<T>& vec) {
    return vec;
}

template<typename T>
std::vector<T> view_result_to_vec(std::optional<T>&& opt) {
    std::vector<T> result;
    if (opt.has_value()) {
        result.push_back(std::move(*opt));
    }
    return result;
}

template<typename T>
std::vector<T> view_result_to_vec(const std::optional<T>& opt) {
    std::vector<T> result;
    if (opt.has_value()) {
        result.push_back(*opt);
    }
    return result;
}

} // namespace Internal
} // namespace SpacetimeDB

#endif // SPACETIMEDB_TEMPLATE_UTILS_H
