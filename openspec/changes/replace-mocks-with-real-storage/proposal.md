# Change: Replace Mock Implementations with Real Storage Backends

## Why
The codebase currently relies heavily on mock implementations (`MockQuotaStorage`, `MockBanStorage`) for testing and in some production paths via `parallel_ban_checker.rs`. This creates several issues:
1. Tests using mocks don't validate real storage behavior
2. `parallel_ban_checker.rs` uses `MockBanStorage` which doesn't persist bans
3. No integration between mock storage and actual `MemoryStorage` implementations
4. Hard to transition from testing to production without code changes

## What Changes
- **src/storage.rs**: Enhance `MemoryStorage` to implement `QuotaStorage` and `BanStorage` traits fully
- **src/ban_manager.rs**: Replace `MockBanStorage` usage with `MemoryStorage` or real storage
- **src/parallel_ban_checker.rs**: Replace `MockBanStorage` with real `BanStorage` implementation
- **src/parallel_ban_checker.rs**: Add production-ready parallel ban checking implementation
- **src/l2_cache.rs**: Ensure L2 cache properly integrates with real storage backends
- **tests/**: Update tests to use real `MemoryStorage` instead of mock implementations

## Impact
- Affected specs: ban-manager, l2-cache, storage
- Affected code:
  - `src/storage.rs`
  - `src/ban_manager.rs`
  - `src/parallel_ban_checker.rs`
  - `src/l2_cache.rs`
  - `tests/common/mod.rs`
  - `tests/common_tests.rs`
- Risk level: Low (replacing mocks with real implementations, no behavioral changes)
- Breaking changes: None (API compatible, just uses real implementations)
