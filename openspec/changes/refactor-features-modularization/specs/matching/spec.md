## MODIFIED Requirements

### Requirement: Identifier Matching System
The system SHALL provide flexible identifier extraction and matching capabilities, including basic matchers (IP, user ID, API key, device ID) that are always available, and optional advanced matchers (geo-location, device fingerprinting) that require explicit feature flags.

#### Scenario: Basic identifier extraction
- **WHEN** a request contains X-User-Id and X-API-Key headers
- **AND** basic matchers are enabled (core features)
- **THEN** UserIdExtractor SHALL extract the user ID
- **AND** ApiKeyExtractor SHALL extract the API key
- **AND** extraction SHALL work without optional features

---

## ADDED Requirements

### Requirement: Geo-Location Matching (Optional)
The system SHALL provide geo-location based matching capabilities when the `geo-matching` feature is enabled, using MaxMindDB for IP geolocation.

#### Scenario: Geo-location matching enabled
- **WHEN** Limiteron is compiled with `features = ["geo-matching"]`
- **AND** a GeoMatcher is configured with a MaxMindDB database
- **AND** a request from IP 203.0.113.1 is made
- **THEN** GeoMatcher SHALL extract the geo-location information
- **AND** geo-location based rules SHALL be applied correctly
- **AND** the database SHALL be loaded into memory

#### Scenario: Geo-location matching disabled
- **WHEN** Limiteron is compiled without the `geo-matching` feature
- **AND** code attempts to use GeoMatcher
- **THEN** the code SHALL compile without errors
- **AND** attempting to create a GeoMatcher SHALL return a FeatureNotEnabled error
- **AND** MaxMindDB dependencies SHALL NOT be compiled

#### Scenario: Geo-location memory footprint
- **WHEN** geo-matching feature is enabled
- **AND** MaxMindDB is loaded
- **THEN** the memory footprint SHALL increase by approximately 10-20 MB
- **AND** when geo-matching is disabled, this memory SHALL NOT be allocated

---

### Requirement: Device Fingerprinting (Optional)
The system SHALL provide device fingerprinting capabilities when the `device-matching` feature is enabled, using Woothee for user agent parsing.

#### Scenario: Device matching enabled
- **WHEN** Limiteron is compiled with `features = ["device-matching"]`
- **AND** a DeviceMatcher is configured
- **AND** a request with User-Agent header is made
- **THEN** DeviceMatcher SHALL extract device information (type, os, browser)
- **AND** device-based rules SHALL be applied correctly
- **AND** the parsed information SHALL be accurate

#### Scenario: Device matching disabled
- **WHEN** Limiteron is compiled without the `device-matching` feature
- **AND** code attempts to use DeviceMatcher
- **THEN** the code SHALL compile without errors
- **AND** attempting to create a DeviceMatcher SHALL return a FeatureNotEnabled error
- **AND** Woothee dependencies SHALL NOT be compiled

---

### Requirement: Custom Matcher Extension
The system SHALL support custom matcher implementations via the CustomMatcher trait, which SHALL be always available regardless of optional features.

#### Scenario: Implementing custom matcher
- **WHEN** a user implements the CustomMatcher trait
- **AND** registers it with the CustomMatcherRegistry
- **THEN** the custom matcher SHALL be available for use
- **AND** the custom matcher SHALL integrate with the matching system
- **AND** optional features (geo, device) SHALL NOT be required

#### Scenario: Custom matcher with only core features
- **WHEN** a custom matcher is implemented
- **AND** Limiteron is compiled with only core features
- **THEN** the custom matcher SHALL compile and work correctly
- **AND** no optional dependencies SHALL be pulled in
