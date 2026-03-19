// Single test binary entry point - includes all smoketests
// We put the tests in a single submodule because if they are at the toplevel then
// they all build and link independently, which takes a lot of linker time.
// This has the unfortunate side effect of requiring that they are all listed in a mod.rs,
// but what can you do ¯\_(ツ)_/¯.
mod smoketests;
