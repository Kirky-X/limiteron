## ADDED Requirements

### Requirement: list_bans Function Decomposition
The system SHALL decompose the `list_bans` function into focused helper methods to improve maintainability and testability.

#### Scenario: Query Condition Building
- **WHEN** `build_list_bans_conditions` is called with a `BanFilter`
- **THEN** it returns a tuple of `(Vec<String>, Vec<String>)` representing SQL conditions and parameters
- **AND** it validates target_type against valid values ["ip", "user", "mac"]
- **AND** it validates target_value length does not exceed 255 characters
- **AND** it escapes LIKE wildcard characters in target_value

#### Scenario: SQL Query Construction
- **WHEN** `build_list_bans_query` is called with conditions, params, and filter
- **THEN** it returns a complete SQL query string with proper SELECT, FROM, WHERE, ORDER BY, LIMIT, and OFFSET clauses
- **AND** it uses parameterized queries to prevent SQL injection

#### Scenario: Result Mapping
- **WHEN** `map_ban_records_to_details` is called with database rows
- **THEN** it returns a `Vec<BanDetail>` with all fields properly mapped
- **AND** it converts target_type strings to appropriate `BanTarget` enum values
- **AND** it creates `BanSource` enum variants based on is_manual flag

### Requirement: list_bans Function Length Constraint
The `list_bans` function SHALL not exceed 100 lines of code after refactoring.

#### Scenario: Function Size Compliance
- **WHEN** the refactored `list_bans` function is analyzed
- **THEN** it contains at most 100 lines of code
- **AND** it delegates implementation details to private helper methods
- **AND** the helper methods follow single responsibility principle
