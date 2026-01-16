## 1. Refactor list_bans Function
- [ ] 1.1 Analyze current `list_bans` function structure (src/ban_manager.rs:710)
- [ ] 1.2 Extract query condition building logic into separate method
- [ ] 1.3 Extract SQL query building logic into separate method
- [ ] 1.4 Extract result mapping logic into separate method
- [ ] 1.5 Verify `list_bans` function is under 100 lines after refactoring
- [ ] 1.6 Add unit tests for extracted helper methods
- [ ] 1.7 Run all ban_manager tests to verify no regression

## 2. Optimize L2 Cache Lock Contention
- [ ] 2.1 Analyze current L2 cache implementation (src/l2_cache.rs)
- [ ] 2.2 Benchmark current implementation under concurrent load
- [ ] 2.3 Evaluate replacing `Mutex<LruCache>` with `DashMap`
- [ ] 2.4 Design migration strategy with backward compatibility
- [ ] 2.5 Implement `DashMap`-based L2 cache if benchmark shows improvement
- [ ] 2.6 Add concurrent access tests for L2 cache
- [ ] 2.7 Run all l2_cache tests to verify no regression

## 3. Validation
- [ ] 3.1 Run `cargo test --lib --all-features` - all tests pass
- [ ] 3.2 Run `cargo check --all-features` - no errors
- [ ] 3.3 Run `cargo clippy --all-features` - no warnings
- [ ] 3.4 Validate OpenSpec change: `openspec validate refactor-code-quality-issues --strict`
