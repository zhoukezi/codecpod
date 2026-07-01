# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-07-01

### Added

- Default FFmpeg log level to quiet.

[0.3.0]: https://github.com/zhoukezi/codecpod/compare/v0.2.3...v0.3.0

## [0.2.3] - 2026-07-01

### Changed

- Add changelog covering releases through 0.2.2.
- Add git-cliff config for changelog generation.

### Fixed

- Handle va_list through a C shim for cross-arch builds.

[0.2.3]: https://github.com/zhoukezi/codecpod/compare/v0.2.2...v0.2.3

## [0.2.2] - 2026-07-01

### Added

- `set_log` to configure FFmpeg's internal logging level.
- `CODECPOD_BUILD_DIR` environment variable to relocate build artifacts.

### Changed

- Added `maturin` to the development dependencies.
- Cleaned up test formatting and dropped an unused NumPy import.

[0.2.2]: https://github.com/zhoukezi/codecpod/compare/v0.2.1...v0.2.2

## [0.2.1] - 2026-06-29

### Changed

- Bumped vendored FFmpeg to 8.1.2, libogg to 1.3.6, and Opus to 1.6.1.

### Fixed

- Use absolute README links so they resolve correctly on PyPI.

[0.2.1]: https://github.com/zhoukezi/codecpod/compare/v0.2.0...v0.2.1

## [0.2.0] - 2026-06-23

### Changed

- Upgraded PyO3 and rust-numpy to 0.29.
- Raised the minimum supported Python version to 3.8.

[0.2.0]: https://github.com/zhoukezi/codecpod/compare/v0.1.2...v0.2.0

## [0.1.2] - 2026-06-23

### Changed

- Release the GIL during audio operations.

[0.1.2]: https://github.com/zhoukezi/codecpod/compare/v0.1.1...v0.1.2

## [0.1.1] - 2026-06-12

### Added

- Workflow to publish the crate to crates.io.

### Changed

- Verify the version matches the tag before publishing to PyPI.
- Updated the maturin-action reference to the PyO3 org after the repository transfer.

### Fixed

- Fixed the docs.rs build.

### Removed

- Dropped the Python 3.13t wheel.
- Dropped TestPyPI publishing and dev version stamping.

[0.1.1]: https://github.com/zhoukezi/codecpod/compare/v0.1.0...v0.1.1

## [0.1.0] - 2026-06-03

### Added

- Initial release: Python bindings for FFmpeg-based audio decoding and encoding.
- Support for building on Windows and macOS.

[0.1.0]: https://github.com/zhoukezi/codecpod/releases/tag/v0.1.0
