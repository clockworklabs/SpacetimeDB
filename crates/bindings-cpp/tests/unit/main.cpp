#include "test_harness.h"

#include <exception>
#include <iostream>

int main(int argc, char** argv) {
    bool verbose = argc > 1 && std::string(argv[1]) == "-v";
    int failures = 0;

    for (const auto& test : SpacetimeDB::UnitTests::all_tests()) {
        try {
            test.func();
            if (verbose) {
                std::cout << "[PASS] " << test.name << '\n';
            }
        } catch (const std::exception& ex) {
            ++failures;
            std::cerr << "[FAIL] " << test.name << ": " << ex.what() << '\n';
        } catch (...) {
            ++failures;
            std::cerr << "[FAIL] " << test.name << ": unknown exception\n";
        }
    }

    if (!verbose) {
        if (failures == 0) {
            std::cout << "Passed " << SpacetimeDB::UnitTests::all_tests().size() << " unit tests\n";
        } else {
            std::cerr << failures << " unit test(s) failed\n";
        }
    }

    return failures == 0 ? 0 : 1;
}
