# M2 real iPhone checklist

Run before calling M2 done, with the user at the iPhone. Record the date, devices and results at the bottom. PC: run `lanpilot-agent run --pair` from a `main` build (Windows), and on Ubuntu when available.

## Before you start

- Build the agent from `main` (the agent work is merged).
- Allow `lanpilot-agent` through the Windows firewall on private networks (UDP 45810).
- Keep the phone and the PC on the same Wi-Fi. Find the PC's IP with `ipconfig`.
- `lanpilot-agent password set` prompts for the password twice.
- Settings is the gear on "Add computer", or "Settings" in the switcher sheet.

## Install

1. From `app/`: `flutter run --release -d <iPhone>` (team NZ7845MU5Y). The app opens on "Add computer".
2. Check the home screen name is "LanPilot" and the app runs at 120 Hz (scrolling the settings list is smooth on ProMotion).

## Local network permission (M0 lessons, spec 4.4)

1. Fresh install. On "Add computer", wait: the iOS Local Network prompt appears (from browsing). Tap Allow. The PC appears under "Nearby computers".
2. Pair (any method). It works without restarting the app. This is the M0 bug that M2 fixes.
3. Delete the app, reinstall, and this time tap "Don't Allow". Pair by pasting a link: after two timeouts (roughly 10 s) it shows "Cannot reach the computer. Make sure both are on the same network.", not a hang. Then in iOS Settings > Privacy & Security > Local Network turn LanPilot on, come back and tap Pair again: it works, still without restarting.
   Variant: tap Pair, and while it is waiting allow access in the iOS prompt. Pairing resets the network socket and retries once after a timeout, so it succeeds without restarting the app.
4. With a PC that was paired and connected before, turn Local Network off for LanPilot in iOS Settings and open the app: it shows "The computer is offline. Retrying..." and after about 5 s the switcher opens. Turn access on and come back: it connects.
5. The "Local network access needed" guide (with "Open Settings" and "Retry") appears only for a paired computer on a phone that has never connected since install and keeps timing out. That is hard to reach by hand and is covered by automated tests. The manual path for the same cause is step 3: fresh install, "Don't Allow", paste a link, expect the "Cannot reach the computer" error above.

## Pairing (spec 1.1)

1. Scan: "Scan QR code", allow the camera, scan the PC's QR code. It pairs and opens the touchpad.
2. System camera: scan the PC's QR code with the iOS Camera app and tap the `lanpilot://` banner. LanPilot opens and pairs.
3. Paste: copy the `lanpilot://pair?d=...` line (Universal Clipboard from the Mac is fine), tap the paste icon, then Pair.
4. Paste junk ("hello"): "This is not a LanPilot pairing link."
5. Paste a pairing link or scan a QR code older than 2 minutes (e.g. an earlier one in the terminal scrollback): "The pairing code expired..." In the scanner, an expired QR code shows a Retry button, and the scanner does not loop on the same code.
6. Password: on the PC `lanpilot-agent password set`, restart the agent. Tap the PC under "Nearby computers", enter the password. Then a wrong password: "Wrong password." Press the keyboard Done twice quickly on the password field: only one pairing starts (the PC counts one attempt, not two).
7. Manual IP: "Enter IP address", type the PC's IP (no port) and the password. It pairs.
8. The PC's device list (`lanpilot-agent devices list`) shows "iPhone" with OS `Ios` (Debug format) after each pairing.

## Connection (spec 4)

1. Kill and reopen the app: it connects to the last PC within about 2 s (up to 1.5 s Bonjour wait plus connect) and opens the touchpad.
2. With the PC agent stopped, open the app: the overlay says the PC is offline and retrying; after about 5 s the switcher opens. Start the agent: it connects by itself.
3. Background: connect, go to the home screen for 10 s, come back: "Reconnecting..." then connected. The agent logs "session started" and "session ended". Compare when "session ended" appears with when you swiped home: within about 1 s is a clean close; about 3 s later is an idle timeout. In that case iOS suspended the app before the close was sent: note it, it means the disconnect needs a background task (`beginBackgroundTask`).
4. Background while holding: hold "Left" on the button strip, swipe to the home screen. On the PC the left button is released (a drag in progress ends).
5. Wi-Fi: turn Wi-Fi off and on in Control Center. It reconnects without touching the app.
6. Two PCs: pair both, switch with the switcher (top name), check the online and offline labels and the check mark.
7. Unpair by swiping left in the switcher; a confirm dialog offers Unpair and Cancel. After confirming, the PC's device list no longer has the phone. Do this on the computer you are currently connected to as well: it works without a glitch.
   Open the switcher right after launch on a slow start: it opens only once.
8. Stop the PC agent with Ctrl-C (or SIGTERM or SIGHUP on Linux; on Windows also close its console window, log off or shut down): the PC leaves "Nearby computers" at once, because the agent sends a Bonjour goodbye. Only a hard kill (Task Manager "End task", `kill -9`) or a crash leaves a stale nearby entry, which disappears after a while.
9. Sleep: the screen stays awake only on the touchpad tab, and only while the PC is connected or being reached. On the Media and Shortcuts tabs and under Settings, auto-lock lets the screen sleep. Leave the phone on the touchpad and put the PC to sleep: once the overlay says the PC is offline, auto-lock lets the phone sleep too.
10. On the PC `lanpilot-agent devices remove <id>`: the phone shows "This computer removed this device. Pair again."
11. Wrong address while connected: while connected to a PC, open "Add computer", choose "Enter IP address" and type an IP where no PC runs (e.g. an unused address on your subnet) with any password. After the "Cannot reach the computer" error, go back: the touchpad still controls the connected PC (move the cursor, click).

## Touchpad (spec 5)

1. One finger moves the cursor; tiny jitter does not move it.
2. Tap: left click after a short delay. Double tap: double click (opens a desktop icon); a slightly wobbly double tap still double clicks.
3. Tap then hold and move: drags a window; lifting releases it.
4. Two-finger tap: context menu. Three-finger tap: middle click (opens a link in a new tab).
5. Two-finger scroll in a long page, both directions; a fling keeps scrolling and a touch stops it. Toggle "Natural scrolling" and check the direction flips.
6. Pinch in a browser: zooms in and out (Windows Ctrl + wheel). Small pinches do not zoom by accident while scrolling. The zoom keeps up with the fingers and stops when they lift (no lag, no runaway zoom).
7. Button strip: hold Left and drag on the pad (selects text). Hide the strip in settings: it disappears.
8. Landscape: the pad fills the screen, buttons on the right, tabs as icons on the left.
9. Haptics: taps tick, drag start bumps; turning haptics off silences both.

## Feel tuning (spec 5.2)

Windows now uses absolute injection, so only the phone's curve matters there. With each acceleration preset (Off, Low, Medium, High) and sensitivity 1.0:

1. Slow precise moves: can you hit a 16 px close button easily?
2. Fast flicks: can you cross a 4K screen in one swipe on High?
3. Pinch zoom: the agent keeps the fractional remainder per session, so up to almost one wheel notch can carry into the next pinch. A first pinch that seems to zoom one step "late" or "early" is this, not a bug.
4. Note preferred values. If the defaults feel wrong, change `app/lib/touchpad/tuning.dart` (basePxPerPt, accelSlowSpeed, accelFastSpeed, accelMaxGain, scrollPtPerNotch, zoomStepsPerDoubling, tap timings) and record the old and new values below.

## Media and the rest

1. Media page: Previous, Play/Pause, Next, Volume down, Mute, Volume up all act on the PC.
2. Shortcuts tab shows "Coming soon".
3. Settings: every option changes behavior immediately; the version under About is 0.2.0.
4. Themes: Native (light and dark, following iOS), Dark, Brand (gradient, frosted pad). Language: Chinese and English, and "System" follows iOS.

## Results

| Date | iPhone / iOS | PC | Result | Notes (tuning values, problems) |
|---|---|---|---|---|
