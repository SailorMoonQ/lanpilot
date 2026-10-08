# M2 real iPhone checklist

Run before calling M2 done, with the user at the iPhone. Record the date, devices and results at the bottom. PC: run `lanpilot-agent run --pair` from the `feat/m2-agent` build (Windows), and on Ubuntu when available.

## Install

1. From `app/`: `flutter run --release -d <iPhone>` (team NZ7845MU5Y). The app opens on "Add computer".
2. Check the home screen name is "LanPilot" and the app runs at 120 Hz (scrolling the settings list is smooth on ProMotion).

## Local network permission (M0 lessons, spec 4.4)

1. Fresh install. On "Add computer", wait: the iOS Local Network prompt appears (from browsing). Tap Allow. The PC appears under "Nearby computers".
2. Pair (any method). It works without restarting the app. This is the M0 bug that M2 fixes.
3. Delete the app, reinstall, and this time tap "Don't Allow". Pair by pasting a link: a clear error appears, not a hang. Pair anyway is impossible; then in iOS Settings > Privacy & Security > Local Network turn LanPilot on, come back and tap Pair again: it works, still without restarting.
   Variant: tap Pair, and while it is waiting allow access in the iOS prompt. Pairing resets the network socket and retries once after a timeout, so it succeeds without restarting the app.
4. With a paired PC and access turned off in Settings: open the app. After a few seconds the touchpad shows the "Local network access needed" guide with "Open Settings" and "Retry". "Open Settings" opens the app's settings page. Turn access on, come back, tap "Retry": it connects.
5. A phone that has never connected and keeps timing out also shows the "Local network access needed" guide.

## Pairing (spec 1.1)

1. Scan: "Scan QR code", allow the camera, scan the PC's QR code. It pairs and opens the touchpad.
2. System camera: scan the PC's QR code with the iOS Camera app and tap the `lanpilot://` banner. LanPilot opens and pairs.
3. Paste: copy the `lanpilot://pair?d=...` line (Universal Clipboard from the Mac is fine), tap the paste icon, then Pair.
4. Paste junk ("hello"): "This is not a LanPilot pairing link."
5. Wait 2 minutes, then paste an old link: "The pairing code expired..." Also scan an expired QR code in the scanner: a Retry button appears and the scanner does not loop on the same code.
6. Password: on the PC `lanpilot-agent password set`, restart the agent. Tap the PC under "Nearby computers", enter the password. Then a wrong password: "Wrong password." Press the keyboard Done twice quickly on the password field: only one pairing starts (the PC counts one attempt, not two).
7. Manual IP: "Enter IP address", type the PC's IP (no port) and the password. It pairs.
8. The PC's device list (`lanpilot-agent devices list`) shows "iPhone" with OS iOS after each pairing.

## Connection (spec 4)

1. Kill and reopen the app: it connects to the last PC within about 1 s and opens the touchpad.
2. With the PC agent stopped, open the app: the overlay says the PC is offline and retrying; after about 5 s the switcher opens. Start the agent: it connects by itself.
3. Background: connect, go to the home screen for 10 s, come back: "Reconnecting..." then connected. The agent log shows a clean close (application close code 0) when the phone went to the background, not an idle timeout 3 s later. If it shows a timeout, iOS suspended the app before the close was sent: note it, it means the disconnect needs a background task (`beginBackgroundTask`).
4. Background while holding: hold "Left" on the button strip, swipe to the home screen. On the PC the left button is released (a drag in progress ends).
5. Wi-Fi: turn Wi-Fi off and on in Control Center. It reconnects without touching the app.
6. Two PCs: pair both, switch with the switcher (top name), check the online and offline labels and the check mark.
7. Unpair by swiping left in the switcher: the PC's device list no longer has the phone. Do this on the computer you are currently connected to as well: it works without a glitch.
   Open the switcher right after launch on a slow start: it opens only once.
8. Stop the PC agent without Ctrl-C (kill it or crash it): the phone may still list it as nearby for a while. This is a stale Bonjour record; the agent side is being fixed in the Windows session.
9. Sleep: the screen stays awake only on the touchpad tab. On the Media and Shortcuts tabs and under Settings, auto-lock lets the screen sleep.
10. On the PC `lanpilot-agent devices remove <id>`: the phone shows "This computer removed this device. Pair again."

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
