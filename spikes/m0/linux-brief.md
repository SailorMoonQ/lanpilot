# M0 spikes on Ubuntu 24.04 + GNOME + Wayland (spikes 1 + 2-Linux)

Throwaway. Work only on branch `spike/m0`; record findings in `spikes/m0/linux-results.md`, commit and push. Conventional Commits, no `Co-Authored-By`, no em dash. Do not modify `core/`, `input/` or `agent/` (a spike may copy code into its own directory).

Prerequisites (once):
- `sudo apt install build-essential pkg-config libssl-dev wl-clipboard ibus python3-gi gir1.2-ibus-1.0` and rustup stable.
- Install the uinput rule: `sudo cp packaging/linux/70-lanpilot-uinput.rules /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger /dev/uinput`, then log out and back in.
- Confirm `echo $XDG_SESSION_TYPE` prints `wayland` and record `gnome-shell --version`.
- Also run the M1 checklist's Ubuntu section (`docs/e2e/m1-checklist.md`) and the ignored live test `cargo test -p lanpilot-input -- --ignored`; record results there too.

## Spike 1: how should the agent type text on GNOME Wayland?

Question: can a background process (no focused window of its own, like the agent) reliably insert arbitrary Unicode text, including Chinese and emoji, into the focused app?

Try, in this order, with a small throwaway program in `spikes/m0/linux-text/`. Each candidate gets a 3 s countdown so you can focus the target app first.

1. **Clipboard + Ctrl+V via uinput.**
   - Set the clipboard from a background process using (a) `wl-copy` (the wl-clipboard CLI) and (b) the `arboard` crate with its Wayland data-control feature.
   - Paste with Ctrl+V; for terminals, also try Ctrl+Shift+V. Reuse `lanpilot_input::linux::UinputBackend` from `input/`.
   - Then restore the previous clipboard content.
   - Record:
     - Does setting the clipboard work without a focused window of our own?
     - Does GNOME (Mutter on 24.04) support the data-control protocol at all?
     - Is the original clipboard restored correctly?
2. **IBus engine.**
   - Write a minimal IBus engine in Python (`IBus.Engine` via `gi`) that commits a given string (for example read from a FIFO or D-Bus) with `commit_text`.
   - Register it, switch to it with `ibus engine <name>`, and inject text.
   - Record:
     - Does it commit into GTK, Qt, Electron/Chromium and terminal apps?
     - What setup does the user need (adding an input source in GNOME settings)?
     - Can the agent switch to it and back without disturbing the user's own IME (for example Chinese Pinyin)?

Targets for every candidate: gedit or GNOME Text Editor, GNOME Terminal (or Console), Firefox, Chrome or VS Code (Electron), and the LibreOffice Writer or Qt app if one is installed.

Test strings: `hello`, `你好，LanPilot`, `emoji 🎉`, a 2000-character paragraph, and text containing a newline.

Deliverable: a recommendation for the agent, including the failure behavior the agent should expect.

## Spike 2 (Linux half): does GNOME pointer acceleration stack with ours?

Question: libinput applies an acceleration profile to relative pointer devices. Is our uinput virtual mouse accelerated, and how much does that distort the motion the phone sends?

1. Build and run the agent: `cargo run --release -p lanpilot-agent -- --home /tmp/lp-spike-agent run --pair`.
2. Pair `lpctl` with it: `cargo run --release -p lpctl -- --home /tmp/lp-spike-ctl pair "<uri>"`.
3. Measure with `spikes/m0/accel/measure.html`. Open it in Firefox with the window maximized. It records cursor displacement for each burst of movement.
4. Run `lpctl move <server> 400 0 --steps N` for N in 100, 40, 10, 4, with 3 repeats each. Put the cursor back in the middle of the page before each run (click the "center" hint).
5. Repeat step 4 with `gsettings set org.gnome.desktop.peripherals.mouse accel-profile 'flat'`, then with `'default'`, then restore the original value.
6. Record the measured displacement and the ratio for each speed and profile. Compare with the Windows results in `spikes/m0/accel/` (the Windows session adds them there).

Deliverable: a recommendation. Options include compensating on the phone, having the agent set its virtual device to a flat profile (is that even possible per device?), or offering a "follow system acceleration" option.
