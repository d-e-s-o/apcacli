Unreleased
----------
- Formatted code base using `rustfmt` and checked in configuration
  - Added enforcement of code formatting style checks in CI


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
