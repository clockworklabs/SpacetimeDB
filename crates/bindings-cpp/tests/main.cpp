#include <iostream>
#include <string_view>

int RunModuleLibraryUnitTests();

int main(int argc, char** argv) {
    const int result = RunModuleLibraryUnitTests();
    if (result == 0 && argc > 1 && std::string_view(argv[1]) == "-v") {
        std::cout << "Module library unit tests passed\n";
    }
    return result;
}
