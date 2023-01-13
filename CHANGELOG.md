Unreleased
----------
- Bumped `apca` dependency to `0.26.0`


0.1.6
-----
- Added support for submission of more kinds of bracket orders
- Migrated over to using `clap` v3 for argument parsing
  - Removed `structopt` dependency
- Switched to using GitHub Actions as CI provider
- Bumped minimum supported Rust version to `1.60`
- Bumped `apca` dependency to `0.25.0`
- Bumped `chrono-tz` dependency to `0.8.1`


0.1.5
-----
- Added support for historic aggregate bar retrieval via `bars`
  subcommand
- Added support for specifying the asset class to use to `asset list`
  subcommand
- Adjusted build script to handle non-existent `git` command or
  repository gracefully
- Bumped `apca` dependency to `0.24.0`


0.1.4
-----
- Removed account update streaming support via `events account`
- Removed `--json` argument from `events` subcommand
- Renamed `events` subcommand to `updates`
- Added support for streaming realtime market data via `updates data`
- Formatted code base using `rustfmt` and checked in configuration
  - Added enforcement of code formatting style checks in CI
- Bumped minimum supported Rust version to `1.56`
- Bumped `apca` dependency to `0.22.0`
- Bumped `tracing-subscriber` dependency to `0.3`


0.1.3
-----
- Added support for generating completion scripts for shells other than
  `bash`
- Added time-in-force column to `order list` command
- Bumped minimum supported Rust version to `1.46`
- Bumped `apca` dependency to `0.20`


0.1.2
-----
- Added support for one-trigger-other order with take-profit leg via
  newly introduced `--take-profit` argument to `order submit` command
- Print textual representation for more account activity types
- Print leg orders in `order get` and `order list` commands
- Bumped minimum supported Rust version to `1.44`
- Bumped `apca` dependency to `0.17`
- Bumped `tokio` dependency to `1.0`
- Bumped `tracing-subscriber` dependency to `0.2`


0.1.1
-----
- Print ID of changed order for `order change` command
- Use default terminal foreground color instead of black for indicating
  no gains/losses
- Enabled CI pipeline comprising building and linting of the project
  - Added badge indicating pipeline status
- Bumped `apca` dependency to `0.15`


0.1.0
-----
- Initial release
