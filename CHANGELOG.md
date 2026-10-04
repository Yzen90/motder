# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.0] - 2026-10-04

### Added

- `add`: add a message from arguments or stdin.
- `list` / `ls`: list messages with indices; `--json` output.
- `dismiss` / `rm`: dismiss messages by index; defaults to the most recent.
- `clear`, `show`, `install` and `uninstall [--purge]`.
- Hook script `/etc/update-motd.d/99-motder` that prints pre-rendered messages.
- Atomic, lock-protected updates.
- Man page, bash completion and `cargo deb` metadata.
