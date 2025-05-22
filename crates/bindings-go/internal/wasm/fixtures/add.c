// use WASI SDK's clang to compile
// clang --target=wasm32 -nostdlib -Wl,--export-all -Wl,--no-entry -o add.wasm add.c

int x = 0;

int add(int a, int b) {
    return a + b + x;
}