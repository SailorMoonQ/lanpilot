# M1 manual end-to-end checklist

Run before calling M1 done. Record the date, machines and results at the bottom.

## Windows 11 (agent) + lpctl on the same or a second machine

1. `cargo build --release -p lanpilot-agent -p lpctl`
2. `target/release/lanpilot-agent run --pair` and allow the firewall prompt (private networks).
3. In another terminal: `target/release/lpctl discover`. The agent appears with its name and IPv4 address.
4. `target/release/lpctl pair "<URI printed by the agent>"`. The agent logs `new device paired: lpctl (...)`.
5. `lpctl square <name>`. The cursor draws a square.
6. `lpctl click <name> right`. A context menu opens at the cursor.
7. `lpctl scroll <name> -3` over a long page. It scrolls down about 3 notches, smoothly.
8. `lpctl key <name> win d`. The desktop is shown.
9. `lpctl media <name> mute` and `vol-up`. The system volume changes.
10. Focus Notepad, then `lpctl text <name> "你好 LanPilot"`. The text appears.
11. `lanpilot-agent devices list` shows lpctl. Then `lanpilot-agent devices remove <id>` while step 5 runs in a loop: the next lpctl command fails with "not paired".
12. Password: `lanpilot-agent password set`, restart the agent, then `lpctl pair-password <ip>:45810`. Five wrong attempts lock pairing (the agent logs the lock warning).

## Ubuntu 24.04 + GNOME + Wayland (agent)

1. `sudo cp packaging/linux/70-lanpilot-uinput.rules /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger /dev/uinput`, then log out and in.
2. `cargo test -p lanpilot-input -- --ignored`. The live uinput test passes.
3. Repeat Windows steps 2 to 9 with `super d` (GNOME overview) instead of `win d`, and from lpctl on the Windows machine.
4. `lpctl text` replies "text input is not supported on this computer" (expected until the M0 spike).
5. Note the pointer feel for the acceleration question in spec 9 (does GNOME's acceleration stack with ours?).

## Results

| Date | Agent machine | Client | Result | Notes |
|---|---|---|---|---|
