# Kanban MVP implementation plan

## Problem and approach

Build the MVP described in the workspace `AGENTS.md` from this repository's current documentation-only skeleton. Use a single idiomatic Dioxus/Rust codebase for web, desktop, and Android, with a Rust SQLite backend and minimal account registration/sign-in. Follow the Dioxus agent guide for current project structure and APIs. Preserve the existing modified `AGENTS.md`.

## Phases and success criteria

### 1. Scaffold and establish the app
- [x] Add the Dioxus/Cargo project structure, target configuration, assets, and `.gitignore`.
- [x] Pin compatible current dependencies and build the web client; check desktop/mobile client targets.
- [x] Keep setup and README concise; document exact run/test commands.

### 2. Implement persistence and account management
- [x] Define SQLite migrations/schema and Rust data access for users, boards, five columns per board, and cards.
- [x] Add registration/sign-in with securely hashed passwords and account-scoped access to stored boards.
- [x] Seed one polished sample board for new accounts, and ensure it is only seeded once.
- [x] Cover validation, authorization boundaries, and core persistence behavior with focused unit/integration tests.

### 3. Build the Kanban experience
- [x] Implement a polished responsive Dioxus interface using the specified color palette and a simple account flow.
- [x] Support multiple boards, renaming the fixed five columns, creating/deleting cards, and moving cards between columns by drag and drop.
- [x] Keep cards limited to title/details and omit archive, search, filters, and other features.
- [x] Add accessible move controls and verify responsive behavior on web and available client targets.

### 4. Verify end-to-end and deliver
- [x] Run formatting, lint/type checks, unit tests, and all feasible target checks; fix regressions.
- [x] Run browser integration tests for registration, seeded data, column rename, card CRUD, drag/drop, and persistence after reload.
- [x] Start the web server and verify it responds; leave it running for the user.
- [x] Build and run the desktop app after installing the required `xdotool` dependency.
- [x] Record platform/toolchain limitations.

### 5. Prepare and test Android on a physical device
- [x] Install a user-local Android SDK, platform tools, platform/build tools, NDK, CMake, JDK 21, and Android Rust targets.
- [x] Configure the user's shell environment for the Android toolchain.
- [x] Build an ARM64 Android APK with the local API address configured for ADB reverse forwarding.
- [x] Connect the USB-debugging phone, install the APK, and launch the app.
- [x] Forward the phone's `127.0.0.1:18080` to the host API at `127.0.0.1:8080` and verify the phone-to-server tunnel.

## Notes

- The user confirmed basic email/password accounts, server-side SQLite persistence, one seeded sample board per new account, no OAuth/password recovery, shared Dioxus codebase for all three targets, and local web verification.
- The repository has no source or Cargo manifests yet. The current `AGENTS.md` is modified in the worktree and must not be overwritten or reverted.
- Dioxus guide consulted: `https://github.com/DioxusLabs/dioxus/blob/main/AGENTS.md`.
- The web release build, desktop release build, and Android ARM64 build pass. The native Android SDK and JDK are installed under the user's home directory; shell configuration is in `~/.bashrc`.
- The Android app is installed and running on the connected Samsung phone. Its test build uses `http://127.0.0.1:18080`; `adb reverse tcp:18080 tcp:8080` connects that address to the local API. Keep the phone's ADB connection active while testing.
- The Android APK was built with `dx build --platform android --target aarch64-linux-android --release` and installed from `target/dx/kanban/release/android/app/app/build/outputs/apk/debug/app-debug.apk`.
- The API responds through the phone tunnel. The app is at its account screen; sign-in/registration and in-app board flows can now be exercised on-device.
- The earlier direct `cargo check --target aarch64-linux-android` did not configure the NDK linker; Dioxus's `dx build` successfully handled the Android build.
