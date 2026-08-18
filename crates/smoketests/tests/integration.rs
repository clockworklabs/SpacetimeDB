// Portable test binary entry point. These smoketests can run against either a
// local standalone server or a remote server.
//
// We group the tests into one binary to avoid linking every source file as an
// independent integration test target.
mod smoketests;
