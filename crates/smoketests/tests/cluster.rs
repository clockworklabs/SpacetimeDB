// Cluster test binary entry point. These smoketests provide useful coverage
// against a cluster, but can also run against a local standalone server.
//
// We group the tests into one binary to avoid linking every source file as an
// independent integration test target.
mod smoketests;
