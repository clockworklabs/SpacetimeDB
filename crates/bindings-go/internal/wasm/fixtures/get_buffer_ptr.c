#include <stdint.h>

// use WASI SDK's clang to compile
// clang --target=wasm32 -nostdlib -Wl,--export-all -Wl,--no-entry -o get_buffer_ptr.wasm get_buffer_ptr.c

uint8_t buffer[16];

uint32_t get_buffer_ptr() {
    return (uint32_t)buffer;
}