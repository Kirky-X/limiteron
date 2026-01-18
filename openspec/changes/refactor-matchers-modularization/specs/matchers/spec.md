## ADDED Requirements

### Requirement: Matchers Module Structure

The system SHALL organize all matcher-related code into a standalone `src/matchers/` module with the following structure:

```
src/matchers/
├── mod.rs      # Core matchers (Identifier extractors, Rule matching engine)
├── geo.rs      # Geo-location matcher
├── device.rs   # Device type matcher
└── custom.rs   # Custom matcher extensions
```

#### Scenario: Module organization
- **GIVEN** the current codebase has `matchers.rs`, `geo_matcher.rs`, `device_matcher.rs`, and `custom_matcher.rs` in the `src/` directory
- **WHEN** the matchers modularization is implemented
- **THEN** all matcher-related files SHALL be moved to `src/matchers/` directory
- **AND** the module structure SHALL follow the pattern above
- **AND** `src/matchers.rs` SHALL be renamed to `src/matchers/mod.rs`

### Requirement: Public API Compatibility

The system SHALL maintain backward compatibility for all public APIs after the matchers modularization.

#### Scenario: Core matcher API unchanged
- **GIVEN** existing code uses `limiteron::Identifier`
- **WHEN** the matchers modularization is implemented
- **THEN** `limiteron::Identifier` SHALL continue to work
- **AND** `limiteron::matchers::Identifier` SHALL also be available as an alternative import path

#### Scenario: RuleMatcher API unchanged
- **GIVEN** existing code uses `limiteron::RuleMatcher`
- **WHEN** the matchers modularization is implemented
- **THEN** `limiteron::RuleMatcher` SHALL continue to work
- **AND** `limiteron::matchers::RuleMatcher` SHALL also be available as an alternative import path

#### Scenario: GeoMatcher API unchanged
- **GIVEN** existing code uses `limiteron::GeoMatcher`
- **WHEN** the matchers modularization is implemented
- **THEN** `limiteron::GeoMatcher` SHALL continue to work
- **AND** `limiteron::matchers::geo::GeoMatcher` SHALL also be available as an alternative import path

#### Scenario: DeviceMatcher API unchanged
- **GIVEN** existing code uses `limiteron::DeviceMatcher`
- **WHEN** the matchers modularization is implemented
- **THEN** `limiteron::DeviceMatcher` SHALL continue to work
- **AND** `limiteron::matchers::device::DeviceMatcher` SHALL also be available as an alternative import path

#### Scenario: CustomMatcher API unchanged
- **GIVEN** existing code uses `limiteron::CustomMatcher`
- **WHEN** the matchers modularization is implemented
- **THEN** `limiteron::CustomMatcher` SHALL continue to work
- **AND** `limiteron::matchers::custom::CustomMatcher` SHALL also be available as an alternative import path

### Requirement: Module Exports

The `src/matchers/mod.rs` SHALL re-export all public types and constants from its sub-modules.

#### Scenario: Core matcher exports
- **GIVEN** the `src/matchers/mod.rs` module defines core matcher types
- **WHEN** the `src/matchers/mod.rs` is implemented
- **THEN** it SHALL re-export `Identifier`, `RequestContext`, `Rule`, `RuleMatcher`, `MatchCondition`, `IdentifierExtractor`, and related types
- **AND** these SHALL be accessible via `limiteron::matchers::<name>` and `limiteron::<name>`

#### Scenario: Geo matcher exports
- **GIVEN** the `src/matchers/geo.rs` module defines `GeoMatcher`, `GeoCondition`, `GeoInfo`, and `GeoCacheStats`
- **WHEN** the `src/matchers/mod.rs` is implemented
- **THEN** it SHALL re-export `GeoMatcher`, `GeoCondition`, `GeoInfo`, and `GeoCacheStats`
- **AND** these SHALL be accessible via `limiteron::matchers::<name>` and `limiteron::<name>`
- **AND** these SHALL be gated by `#[cfg(feature = "geo-matching")]`

#### Scenario: Device matcher exports
- **GIVEN** the `src/matchers/device.rs` module defines `DeviceMatcher`, `DeviceCondition`, `DeviceInfo`, `DeviceType`, and `DeviceCacheStats`
- **WHEN** the `src/matchers/mod.rs` is implemented
- **THEN** it SHALL re-export `DeviceMatcher`, `DeviceCondition`, `DeviceInfo`, `DeviceType`, and `DeviceCacheStats`
- **AND** these SHALL be accessible via `limiteron::matchers::<name>` and `limiteron::<name>`
- **AND** these SHALL be gated by `#[cfg(feature = "device-matching")]`

#### Scenario: Custom matcher exports
- **GIVEN** the `src/matchers/custom.rs` module defines `CustomMatcher`, `CustomMatcherRegistry`, `HeaderMatcher`, and `TimeWindowMatcher`
- **WHEN** the `src/matchers/mod.rs` is implemented
- **THEN** it SHALL re-export `CustomMatcher`, `CustomMatcherRegistry`, `HeaderMatcher`, and `TimeWindowMatcher`
- **AND** these SHALL be accessible via `limiteron::matchers::<name>` and `limiteron::<name>`

### Requirement: Conditional Compilation

The system SHALL use conditional compilation for matchers that depend on external libraries.

#### Scenario: Geo matcher conditional compilation
- **GIVEN** the `GeoMatcher` depends on the `maxminddb` library
- **WHEN** the matchers modularization is implemented
- **THEN** the `geo` module SHALL be gated by `#[cfg(feature = "geo-matching")]`
- **AND** all geo matcher exports SHALL be gated by `#[cfg(feature = "geo-matching")]`

#### Scenario: Device matcher conditional compilation
- **GIVEN** the `DeviceMatcher` depends on the `woothee` library
- **WHEN** the matchers modularization is implemented
- **THEN** the `device` module SHALL be gated by `#[cfg(feature = "device-matching")]`
- **AND** all device matcher exports SHALL be gated by `#[cfg(feature = "device-matching")]`

### Requirement: Test Compatibility

The system SHALL update all test files to use the new module paths while maintaining test functionality.

#### Scenario: Unit tests in matcher modules
- **GIVEN** unit tests in `src/matchers/mod.rs`, `src/matchers/geo.rs`, `src/matchers/device.rs`, and `src/matchers/custom.rs`
- **WHEN** the matchers modularization is implemented
- **THEN** all unit tests SHALL continue to pass
- **AND** no test code SHALL be modified (internal imports within the same module)

#### Scenario: Integration tests
- **GIVEN** integration tests in `tests/` directory that use matcher modules
- **WHEN** the matchers modularization is implemented
- **THEN** all integration tests SHALL be updated to use the new import paths
- **AND** all integration tests SHALL continue to pass

#### Scenario: Example code
- **GIVEN** example code in `examples/` directory that uses matcher modules
- **WHEN** the matchers modularization is implemented
- **THEN** all example code SHALL be updated to use the new import paths
- **AND** all examples SHALL continue to compile and run

### Requirement: Documentation Updates

The system SHALL update project documentation to reflect the new module structure.

#### Scenario: IFLOW.md update
- **GIVEN** the `IFLOW.md` file contains project structure documentation
- **WHEN** the matchers modularization is implemented
- **THEN** `IFLOW.md` SHALL be updated to reflect the new `src/matchers/` module structure
- **AND** the file list SHALL show `src/matchers/mod.rs`, `src/matchers/geo.rs`, `src/matchers/device.rs`, and `src/matchers/custom.rs`

#### Scenario: API reference update
- **GIVEN** the `docs/API_REFERENCE.md` file contains API documentation
- **WHEN** the matchers modularization is implemented
- **THEN** `docs/API_REFERENCE.md` SHALL be updated to reflect the new module paths
- **AND** all matcher-related API examples SHALL use the correct import paths

## MODIFIED Requirements

### Requirement: Source Code Organization

The system SHALL organize source code into logical modules with clear boundaries and responsibilities.

**Previous**: Source code files are organized in a flat structure in `src/` directory.

**Modified**: Source code SHALL be organized into logical modules. Core modules SHALL be in `src/` directory, and related functionality SHALL be grouped into sub-modules (e.g., `src/cache/`, `src/matchers/`, `src/factory/`, `src/bin/`).

#### Scenario: Matchers module organization
- **GIVEN** the project has matcher-related functionality
- **WHEN** organizing source code
- **THEN** matcher-related code SHALL be grouped into `src/matchers/` module
- **AND** the module SHALL contain `mod.rs`, `geo.rs`, `device.rs`, and `custom.rs`
- **AND** the module SHALL be declared in `src/lib.rs` with `pub mod matchers;`

#### Scenario: Module boundary clarity
- **GIVEN** multiple modules in the project
- **WHEN** organizing source code
- **THEN** each module SHALL have a clear responsibility
- **AND** module boundaries SHALL be explicit through module declarations
- **AND** dependencies between modules SHALL be clearly visible through use statements