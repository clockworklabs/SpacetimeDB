//! STDB module used for benchmarks based on "realistic" workloads.
//!
//! This C++ module provides equivalence with the Rust and C# benchmark modules:
//! - Various table definitions with different indexing strategies
//! - Realistic game workload simulations (circles, ia_loop)
//! - Synthetic benchmarking operations
//! - Cross-joins, bulk operations, and performance testing
//!
//! The module is organized into three main benchmark categories:
//! - circles: Game-like entities with spatial queries
//! - ia_loop: AI agent simulation with complex state management
//! - synthetic: Pure database operations with various indexing strategies

#include "common.h"
