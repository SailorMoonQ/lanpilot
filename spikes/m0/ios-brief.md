# M0 spike: lanpilot-core on a real iPhone (spikes 3 + 4)

Throwaway. Nothing here merges into `main`; only the findings in `ios-results.md` are kept.

## Questions to answer

1. Does `lanpilot-core` (quinn 0.11, rustls 0.23 with ring, prost, spake2, mdns-sd) build for `aarch64-apple-ios` and run on a real iPhone through flutter_rust_bridge v2?
2. Does the iOS local network permission prompt appear, and does QUIC over the LAN work after allowing it?
3. End-to-end latency: control-stream round trip from the iPhone to the Windows agent over Wi-Fi (median, P95, max over 200 rounds), plus connect + session setup time.

## Ground rules

- Work only on branch `spike/m0`. Never push to `main`. Commit messages: Conventional Commits, no `Co-Authored-By` line, no em dash anywhere.
- Put everything under `spikes/m0/ios_app/` (Flutter project, with its Rust bridge crate inside, e.g. `spikes/m0/ios_app/rust/`).
- The bridge crate depends on core by path: `lanpilot-core = { path = "../../../../core" }` (adjust to the actual relative path). Do NOT add the spike crate to the root workspace `members`; give it its own `[workspace]` table so it builds standalone.
- Do not modify `core/`. If core does not build for iOS, record exactly why in the results; a minimal patch is acceptable only on this branch and must be described in the results.
- Signing: the user's paid Apple developer account (select the team in Xcode). Bundle id e.g. `dev.lanpilot.spike`.

## The Windows agent (already provided)

The LanPilot session on the Windows PC (`LanPilot`, reachable via Remote Control messaging) runs `lanpilot-agent run --pair` with real input on `192.168.50.203:45810` and will send you a fresh `lanpilot://pair?d=...` URI when you ask (tokens expire after 120 s, so ask right before pairing). Message it when you are ready to pair, and again when done.

## What the app does (one screen, minimal UI)

1. Text field to paste the pairing URI, and a **Pair** button: `Invite::from_uri`, then `lanpilot_core::pairing::flow::pair_with_invite(&endpoint, &invite, "iPhone spike", Os::Ios)`. Persist the client identity secret (32 bytes) and the paired server (public key, addrs, port) in the app documents directory as a plain file (spike only).
2. **Connect** button: `lanpilot_core::session::connect_paired(&endpoint, addr, &server_key)` then `open_session(&conn, &local_hello("iPhone spike", Os::Ios, "0.0.1", &[]))`. Show connect + session setup time in ms.
3. **Draw square** button: send `PointerDatagram` datagrams with cumulative `GestureState` totals tracing a 200 px square (4 sides x 30 steps, 8 ms apart, one `gesture_id`, `seq` from 1). The user should see the Windows cursor draw a square.
4. **Latency test** button: 200 rounds of `ClientMessage { request_id: n, body: RunCommand { command_id: "ping" } }` on the control stream, each awaiting its reply (the agent answers `Error{UNSUPPORTED}` immediately; no side effects). Use `lanpilot_core::framing::{write_msg, read_msg}`. Report min / median / P95 / max in ms. Run it twice: phone close to the router, and phone in another room.
5. Show errors verbatim in the UI.

Useful references: `tools/lpctl/src/lib.rs` (a complete Rust client doing exactly pair / connect / datagrams / requests), `core/src/session.rs`, `core/src/pairing/flow.rs`.

## iOS configuration

- `Info.plist`: `NSLocalNetworkUsageDescription` (e.g. "LanPilot connects to your computer on the local network.") and `NSBonjourServices` = `["_lanpilot._udp"]`.
- No multicast entitlement (we are not using raw mDNS on the phone).
- Tokio runtime: flutter_rust_bridge v2 default async support is fine; make sure the QUIC endpoint lives in a long-lived Rust object (e.g. a static `OnceLock` holding a tokio runtime + endpoint + connection) so it is not dropped between calls.

## Record in `spikes/m0/ios-results.md` (then commit + push to `spike/m0`)

- Versions: macOS, Xcode, Flutter, Rust, flutter_rust_bridge, iOS, iPhone model.
- Build: did core build for iOS unchanged? Every error hit and how it was fixed (or not).
- Release app size (IPA or .app) and the Rust static lib size.
- Permission prompt: did it appear, when, and does denying it produce a clear error?
- Pairing: success? time taken?
- Connect + session setup time (ms), 5 samples.
- Draw square: confirm with the Windows session that the cursor moved (it will tell you).
- Latency: two runs (near / far), min / median / P95 / max.
- Anything surprising (background/foreground behavior, Wi-Fi sleep, crashes).
- Your recommendation: is flutter_rust_bridge + core the right path for M2? Any changes needed in core for iOS?

When done, message the `LanPilot` session with a short summary and the commit SHA.
