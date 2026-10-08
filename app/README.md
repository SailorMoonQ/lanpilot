# LanPilot iOS app (M2)

Flutter app that controls a computer running `lanpilot-agent`. Design: `docs/superpowers/specs/2026-10-08-m2-ios-design.md`.

## Layout

- `rust/`: the bridge crate (flutter_rust_bridge v2), a standalone workspace that depends on `../../core`. It pins `libc = "=0.2.189"`; see the comment in `rust/Cargo.toml`.
- `packages/lanpilot_discovery/`: Bonjour browsing as a local plugin. It browses with NWBrowser but resolves services with dns_sd (`DNSServiceResolve` + `DNSServiceGetAddrInfo`), because NWConnection resolution could stall on interface-scoped answers.
- `lib/`: Dart. Everything talks to the bridge through `LanPilotClient` (`lib/bridge/lanpilot_client.dart`), so tests use `test/support/fake_client.dart`.
- Touchpad feel constants: `lib/touchpad/tuning.dart`.

## Setup

- Flutter 3.47.5, Rust stable with `aarch64-apple-ios` and `aarch64-apple-ios-sim`, and `cargo install flutter_rust_bridge_codegen --version 2.13.0 --locked`.
- After changing `rust/src/api/`: `flutter_rust_bridge_codegen generate`, then `cargo fmt` in `rust/` and `dart format lib`. After changing `lib/l10n/*.arb`: `flutter gen-l10n`. Commit the generated files.

## Checks

```
(cd rust && cargo fmt --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked)
dart format --output=none --set-exit-if-changed lib test integration_test
flutter analyze
flutter test                      # goldens run on macOS only
flutter test --update-goldens test/golden   # after intended UI changes
tool/sim_e2e.sh                   # simulator against a real agent on this Mac
```

Golden tests run only on macOS (`Platform.isMacOS`); on other hosts they are skipped. After an intended UI change run the update command on a Mac and commit the changed PNGs in `test/golden/goldens`.

## Simulator end-to-end

`tool/sim_e2e.sh [udid]` needs a booted iOS simulator (or its udid as the argument). It builds the agent and the app first (the build can outlast the 120 s pairing token), starts its own agent with mock input, runs `integration_test/e2e_test.dart` against it, stops the agent with SIGINT (so it withdraws its mDNS record), then checks the agent log for pointer motion, a left button down and Play/Pause. About 30 s warm. On failure it prints the agent log path.

## CI

`.github/workflows/app.yml` runs on macOS for pushes to main and for pull requests touching `app/`, `core/`, `agent/`, `input/`, the root Cargo files, the toolchain file or the workflow. It checks: bridge crate fmt, clippy and tests; that generated code (frb, l10n) is up to date; Dart format, analyze and tests (including the discovery plugin and goldens); and an unsigned `flutter build ios`. Golden diffs are uploaded as the `golden-failures` artifact.

## Running against a Mac agent

From the repository root: `RUST_LOG=info,lanpilot_input=debug cargo run -p lanpilot-agent -- --home "$(mktemp -d)" run --pair --mock-input`. It prints a pairing link and logs every input it receives. Run the app on a simulator (`flutter run`) and paste the link.

Real-device testing: `docs/e2e/m2-checklist.md`.
