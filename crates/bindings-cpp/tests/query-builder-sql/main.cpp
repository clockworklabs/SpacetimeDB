#include <iostream>

int RunQueryBuilderSqlTests();

int main() {
    const int result = RunQueryBuilderSqlTests();
    if (result == 0) {
        std::cout << "All query-builder SQL tests passed\n";
    }
    return result;
}
