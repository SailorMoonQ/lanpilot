# M0 spike results: lanpilot-core on a real iPhone (spikes 3 + 4)

Run on 2026-10-08. App: `spikes/m0/ios_app/` (Flutter, bridge crate in `ios_app/rust/`).

## Versions

| Item | Version |
|---|---|
| macOS | 26.6 (25G72) |
| Xcode | 27.0 (27A266a) |
| Flutter / Dart | 3.47.5 stable / 3.13.4 |
| Rust | 1.99.0 stable (installed via rustup for this spike; none was on the Mac) |
| flutter_rust_bridge | 2.13.0 (codegen and runtime) |
| CocoaPods | 1.17.0 |
| Locked crates | quinn 0.11.12, rustls 0.23.45 (ring) |
| iPhone | iPhone 17 Pro Max (iPhone18,2), iOS 26.6.1 (23G83) |
| Agent | Windows PC `DESKTOP-TTO9F0C`, 192.168.50.203:45810, wired; phone on Wi-Fi in the same /24 |

## Build

- `lanpilot-core` built for `aarch64-apple-ios` unchanged (`cargo build -p lanpilot-core --target aarch64-apple-ios --release`, 24 s clean). No patch to `core/`.
- The only build error was outside core: `backtrace` (pulled in by flutter_rust_bridge via `allo-isolate`) failed with `cannot find function _dyld_image_count in crate libc` (plus 3 more `_dyld_*`). libc 0.2.190 gates those functions to `target_os = "macos"`; 0.2.189 and earlier still expose them on iOS. Bumping backtrace to 0.3.76 did not help. Fix: pin `libc = "=0.2.189"` in the bridge crate. M2 needs this pin (or a newer backtrace/libc pair) until upstream fixes it.
- Bridge crate setup: `flutter_rust_bridge_codegen integrate` (cargokit), then the crate got its own empty `[workspace]` table so it builds standalone, and `lanpilot-core = { path = "../../../../core" }`. `cargo expand` is not installed; codegen warns about it but works.
- Flutter warns that the generated cargokit plugin does not support Swift Package Manager ("will become an error in a future version of Flutter"). Worth tracking for M2.
- Signing: automatic, team NZ7845MU5Y, bundle id `dev.lanpilot.spike`. `flutter build ios --release` worked on the first try (about 60 s with a warm Rust cache).

## Size (release)

| Artifact | Size |
|---|---|
| `Runner.app` | 25.3 MB on disk, 10.4 MB zipped (rough IPA proxy; no IPA exported) |
| Rust framework binary (`rust_lib_lanpilot_spike.framework`) | 10.1 MB |
| Rust static lib (`librust_lib_lanpilot_spike.a`, before linking) | 36.0 MB |
| Flutter.framework / App.framework | 10 MB / 5.2 MB |

Release profile used `strip = "debuginfo"` only; no LTO or `opt-level = "z"`, so the Rust part can likely shrink.

## Local network permission

- Info.plist has `NSLocalNetworkUsageDescription` and `NSBonjourServices = ["_lanpilot._udp"]`; no multicast entitlement.
- The user allowed the prompt during the first app session, but I did not see exactly when it appeared.
- **Key finding:** in that first app session, every QUIC pairing attempt after the prompt timed out (`pair: timed out (Transport(Connection(TimedOut)))`, 3 attempts), even though a raw UDP probe from a fresh socket "sent" fine. The agent (debug logging) saw nothing from the phone. The user changed nothing except **restarting the app**; after that the first attempt reached the agent at once. Likely cause: the quinn endpoint's UDP socket is bound at app start (`client_endpoint` in `init_client`), before the permission exists, and iOS keeps that socket blocked for the life of the process. Not proven (would need a reinstall to reset the permission and retest).
- Denying the permission was not tested. From the above, a blocked socket shows up as a plain QUIC `TimedOut`, not a clear "permission denied" error, so the app cannot tell the user why. M2 needs to detect this itself (e.g. trigger the prompt early with a Bonjour browse, and create or `rebind` the QUIC endpoint only after access is granted, or retry with a fresh socket after a timeout).

## Pairing

- Success: `paired in 30.4 ms` (QR invite flow, `Os::Ios`). Agent logged "new device paired: iPhone spike (b3f9e4cf86ed7e36)".
- A stale invite gave a clear error: `pairing rejected: BadToken`.
- The invite advertises two addresses, `192.168.50.203` and `198.18.0.1`. The second is the PC's Mihomo (Clash) TUN adapter (198.18.0.0/15 fake-IP range). It was harmless here because the LAN address came first, but the agent should not advertise TUN or VPN addresses.

## Connect + session setup

`connect_paired` + `open_session`, 10 samples (all via 192.168.50.203):

| # | connect ms | session ms | total ms |
|---|---|---|---|
| 1 | 18.4 | 5.6 | 24.0 |
| 2 | 17.8 | 7.7 | 25.5 |
| 3 | 18.7 | 6.9 | 25.6 |
| 4 | 6.0 | 6.5 | 12.5 |
| 5 | 18.4 | 7.2 | 25.7 |
| 6 | 8.7 | 6.3 | 14.9 |
| 7 | 16.7 | 6.1 | 22.8 |
| 8 | 12.2 | 6.1 | 18.3 |
| 9 | 12.6 | 6.5 | 19.1 |
| 10 | 15.4 | 5.4 | 20.9 |

Range 12.5 to 25.7 ms; median about 21 ms.

## Draw square

120 `PointerDatagram`s (4 sides x 30 steps, one gesture, seq 1..120) sent in 1167 ms. **The user confirmed the Windows cursor drew the square.** The 8 ms sleep came out at about 9.7 ms per step on iOS (timer overhead); the M2 sender should pace against a clock, not a fixed sleep.

## Latency (control stream, 200 rounds of `RunCommand{"ping"}` -> `Error{UNSUPPORTED}`)

| Run | min | median | P95 | max | mean (ms) |
|---|---|---|---|---|---|
| Near the router | 3.01 | 4.02 | 5.08 | 12.30 | 4.16 |
| Another room | 2.94 | 4.02 | 4.88 | 12.82 | 4.13 |

No errors or timeouts in 400 rounds. The two runs are almost identical, so the "far" room probably still had a strong signal from the same AP; this does not measure a weak-signal case.

## Other observations

- **Background kills the connection.** Connect, go to the home screen for about 10 s, return, Draw square: `connection closed: timed out`. With `IDLE_TIMEOUT` = 3 s and the app suspended, keep-alives stop and quinn times out. Expected; M2 must reconnect (and reopen the session) on every return to the foreground.
- No crashes. The long-lived tokio runtime in a static `OnceLock` plus a `tokio::sync::Mutex<Option<Client>>` worked well with frb async functions (`runtime().spawn(fut).await`).

## Recommendation

**Yes, flutter_rust_bridge v2 + lanpilot-core is the right path for M2.** Core needs no changes to build or run on iOS, the frb async bridge was simple, and the numbers are good (pairing about 30 ms, connect about 20 ms, control round trip median 4 ms / P95 5 ms).

Needed for M2:

1. Pin libc (or move to a fixed backtrace/libc) in the app crate.
2. Local network handling in the app: trigger the prompt early, create or rebind the QUIC endpoint after the permission is granted, and turn handshake timeouts into a "check Local Network permission" hint.
3. Reconnect on foreground; treat the connection as gone whenever the app is backgrounded.
4. Agent or core (`Invite` creation on the server side): skip TUN, VPN and 198.18.0.0/15 addresses in invites and mDNS ads.
5. Optional: size tuning (LTO, `opt-level`, `panic = "abort"`) for the Rust framework, and keep an eye on Flutter's Swift Package Manager requirement for cargokit plugins.
