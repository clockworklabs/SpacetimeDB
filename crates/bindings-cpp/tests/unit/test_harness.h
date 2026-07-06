#ifndef SPACETIMEDB_TEST_HARNESS_H
#define SPACETIMEDB_TEST_HARNESS_H

#include <stdexcept>
#include <string>
#include <vector>

namespace SpacetimeDB::UnitTests {

struct TestCase {
    const char* name;
    void (*func)();
};

inline std::vector<TestCase>& all_tests() {
    static std::vector<TestCase> tests;
    return tests;
}

struct TestRegistrar {
    TestRegistrar(const char* name, void (*func)()) {
        all_tests().push_back(TestCase{name, func});
    }
};

} // namespace SpacetimeDB::UnitTests

#define TEST_CASE(name) \
    void name(); \
    static ::SpacetimeDB::UnitTests::TestRegistrar name##_registrar(#name, &name); \
    void name()

#define ASSERT_TRUE(condition) \
    do { \
        if (!(condition)) { \
            throw std::runtime_error(std::string("Assertion failed: ") + #condition); \
        } \
    } while (0)

#define ASSERT_EQ(expected, actual) \
    do { \
        auto expected_value = (expected); \
        auto actual_value = (actual); \
        if (!(expected_value == actual_value)) { \
            throw std::runtime_error(std::string("Assertion failed: ") + #expected " == " #actual); \
        } \
    } while (0)

#endif // SPACETIMEDB_TEST_HARNESS_H
