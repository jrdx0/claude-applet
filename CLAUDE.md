# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A COSMIC desktop applet for tracking Claude's account plan usage. Built with Rust using the libcosmic framework for the COSMIC desktop environment.

## Build Commands

This project uses [just](https://github.com/casey/just) as its command runner. All commands below use `just`:

- `just` or `just build-release` - Build the application with release profile
- `just build-debug` - Build with debug profile
- `just run` - Build and run the application (sets RUST_BACKTRACE=full)
- `just check` - Run clippy with pedantic warnings
- `just check-json` - Run clippy with JSON output (for IDE integration)
- `just clean` - Run `cargo clean`
- `just install` - Install the applet to the system (use `rootdir` and `prefix` variables to customize paths)
- `just uninstall` - Remove installed files

### Vendored Builds

For packaging or offline builds:

- `just vendor` - Create vendored dependencies in a tarball
- `just build-vendored` - Build with vendored dependencies (frozen, offline)

## Architecture

### Application Structure

This is a COSMIC applet built using the Elm-like architecture pattern provided by libcosmic:

- **main.rs** - Entry point that initializes i18n and starts the applet runtime
- **app.rs** - Core application logic using the Model-View-Update pattern:
  - `AppModel` - Application state (core, popup window ID, config, UI state)
  - `Message` enum - All possible application events
  - `cosmic::Application` trait implementation - Defines view rendering, update logic, and subscriptions
- **config.rs** - Configuration persistence using cosmic-config with CosmicConfigEntry derive macro
- **i18n.rs** - Fluent-based localization system with `fl!()` macro for translations

### COSMIC Applet Pattern

The app follows the COSMIC applet architecture:

- `view()` method renders the panel icon button
- `view_window()` method renders the popup window content when toggled
- Popup management via `TogglePopup` message using libcosmic's popup utilities
- Configuration changes are watched via subscription and auto-update the app state
- Uses async subscriptions for long-running background tasks

### Key Dependencies

- **libcosmic** - COSMIC desktop framework (git dependency from pop-os/libcosmic)
  - Features: applet, applet-token, dbus-config, multi-window, tokio, wayland, winit
- **i18n-embed** + **rust-embed** - Localization system embedding translation files
- **tokio** - Async runtime (full features enabled)

## Localization

Uses Fluent for i18n. Translation files are in `i18n/` directory organized by ISO 639-1 language codes.

- Fallback language is English (en)
- Add translations by copying `i18n/en/` to a new language code directory
- Access translations using the `fl!()` macro: `fl!("message-id")` or `fl!("message-id", arg1, arg2)`
- New languages are auto-detected via i18n-embed's desktop language requester

## Application ID

The app uses RDNN identifier: `com.github.jrdx0.ClaudeApplet`

This is used for:
- Desktop entry and app metadata files
- Configuration storage via cosmic-config
- Resource file naming conventions

## Custom Icon

The applet uses a custom SVG icon located at `resources/icon.svg` (a sparkle/starburst design).

The icon is displayed in the COSMIC panel via `core.applet.icon_button(Self::APP_ID)` in `app.rs:96`. libcosmic automatically looks up the icon using the APP_ID.

When installed via `just install`, the icon is copied to `/usr/share/icons/hicolor/scalable/apps/com.github.jrdx0.ClaudeApplet.svg`. The applet must be installed before running to ensure the icon displays correctly in the panel.

## Development Setup

Install rustup and configure your editor to use rust-analyzer. The project requires Rust 2024 edition.

### Local libcosmic Development

To test against a local libcosmic clone, uncomment the `[patch.'https://github.com/pop-os/libcosmic']` section in Cargo.toml and adjust the paths.
