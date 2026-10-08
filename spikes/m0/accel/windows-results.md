# M0 spike 2 (Windows half): pointer acceleration and injected relative moves

Date: 2026-10-08. Machine: Windows 11 Pro (10.0.26200), primary display, pointer speed slider 10/20 (1:1 position). Agent: `lanpilot-agent` (M1, real `SendInput` backend) on the same PC; driver `lpctl move <server> 400 0 --steps N` (8 ms between datagrams); cursor position read with `GetCursorPos` via `spikes/m0/accel/measure-windows.ps1`. Raw data: `windows-accel-on.csv`, `windows-accel-off.csv`.

## Results (400 px sent per run, 3 repeats each)

| px per frame (8 ms) | Enhance pointer precision ON | OFF |
|---|---|---|
| 4 (slow) | 287 to 299 px (0.72x to 0.75x) | 400 px (1.00x) |
| 10 | 424 to 425 px (1.06x) | 400 px (1.00x) |
| 40 | 846 to 864 px (2.12x to 2.16x) | 400 px (1.00x) |
| 100 (fast) | 990 px (2.48x) | 400 px (1.00x) |

Repeats agree within a few pixels, so the effect is deterministic.

## Findings

- Windows applies its "Enhance pointer precision" curve to relative moves injected with `SendInput` (`MOUSEEVENTF_MOVE`), exactly as it would to a physical mouse. With it on (the Windows default), slow motion shrinks to about 0.73x and fast motion grows to about 2.5x.
- With it off, injected motion is exactly 1:1, so `PointerApplier` and the backend are lossless.
- If the phone also applies its own acceleration curve (spec 6.5 "加速度曲线"), the two curves multiply. Fast swipes would overshoot badly and slow, precise motion would feel sluggish. This is the "叠加" risk in spec 9, and it is real on Windows with default settings.

## Recommendation (Windows)

Inject pointer motion so that the phone alone defines the feel:

- Preferred: keep the wire protocol (relative cumulative totals) but have the Windows backend apply each delta as an absolute move. Read the current position with `GetCursorPos`, add the delta, clamp it to the virtual desktop, then call `SendInput` with `MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK` using 0 to 65535 normalized coordinates.
  - Absolute moves bypass the acceleration curve, so the feel is identical for every user setting, and the phone's sensitivity and acceleration settings become the single source of truth.
  - Needs verification: rounding at 65535 normalization on high-DPI and multi-monitor layouts (sub-pixel error must not accumulate; keep the target in physical pixels and recompute from `GetCursorPos` each time); per-monitor DPI awareness of the agent process (declare it in the manifest or call `SetProcessDpiAwarenessContext`); and behavior when another input device moves the cursor at the same time (reading the position each time handles this).
- Alternative: a "follow system acceleration" toggle that skips phone-side acceleration and sends linear motion. This is simpler, but the feel then depends on each PC's settings, and with "Enhance pointer precision" on, slow motion loses about 25%, which makes precise pointing harder.

Proposed default: absolute injection on Windows, with phone-side acceleration on. Revisit after the Linux half and after a hands-on feel test with the real phone app in M2.
