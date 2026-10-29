# Gather

A minimal Kanban workspace built with Dioxus and Rust. The web, desktop, and Android clients share a SQLite-backed Rust API. New accounts start with one sample board and five columns.

## Run locally

Install the Dioxus CLI 0.7.10 and build the web client:

```sh
cargo install dioxus-cli --version 0.7.10 --locked
dx build --platform web --release
```

Run the app server:

```sh
cargo run --no-default-features --features server --bin kanban-server
```

Open <http://127.0.0.1:8080>. The SQLite database is created at `data/kanban.db`.

The server reads the built frontend from `target/dx/kanban/release/web/public`. Set `KANBAN_WEB_DIR` to use another build directory.

To run a native client against the API, build it with `KANBAN_API_URL` set to the server's origin. Android devices and emulators need an address they can reach.

Set `KANBAN_COOKIE_SECURE=true` when serving over HTTPS.

## Android

Install the Android SDK platform tools, Android platform/build tools, NDK, CMake, JDK 21, and the Rust target:

```sh
rustup target add aarch64-linux-android
```

Set `JAVA_HOME`, `ANDROID_HOME`, and `ANDROID_NDK_HOME` to the installed toolchain paths. For a USB-connected device with USB debugging enabled, `adb reverse` lets the app use the host's loopback API without exposing the server to the network. Start the server, connect and authorize the device, then run:

```sh
adb reverse tcp:18080 tcp:8080
KANBAN_API_URL=http://127.0.0.1:18080 dx build --platform android --target aarch64-linux-android --release
adb install -r target/dx/kanban/release/android/app/app/build/outputs/apk/debug/app-debug.apk
adb shell monkey -p com.joelrealubit.gather -c android.intent.category.LAUNCHER 1
```

Keep the USB/ADB connection and reverse mapping active while testing. This local HTTP setup is for development; builds intended for other networks need an HTTPS API origin reachable by the device. Android builds require `KANBAN_API_URL`.

## Check

```sh
cargo fmt --check
cargo test --no-default-features --features server
cargo clippy --no-default-features --features server --all-targets -- -D warnings
```
