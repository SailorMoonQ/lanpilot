// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get appTitle => 'LanPilot';

  @override
  String get tabTouchpad => 'Touchpad';

  @override
  String get tabShortcuts => 'Shortcuts';

  @override
  String get tabMedia => 'Media';

  @override
  String get statusConnecting => 'Connecting...';

  @override
  String get statusReconnecting => 'Reconnecting...';

  @override
  String get statusOffline => 'The computer is offline. Retrying...';

  @override
  String get statusRemoved => 'This computer removed this device. Pair again.';

  @override
  String get statusUpdateComputer => 'Please update LanPilot on the computer.';

  @override
  String get statusUpdateApp => 'Please update the app.';

  @override
  String get pairAgain => 'Pair again';

  @override
  String get localNetworkTitle => 'Local network access needed';

  @override
  String get localNetworkBody =>
      'Turn on LanPilot in Settings > Privacy & Security > Local Network.';

  @override
  String get openSettings => 'Open Settings';

  @override
  String get retry => 'Retry';

  @override
  String get switcherTitle => 'Computers';

  @override
  String get online => 'Online';

  @override
  String get offline => 'Offline';

  @override
  String get unpair => 'Unpair';

  @override
  String unpairConfirm(String name) {
    return 'Unpair \"$name\"?';
  }

  @override
  String get cancel => 'Cancel';

  @override
  String get addComputer => 'Add computer';

  @override
  String get settings => 'Settings';

  @override
  String get addComputerHint =>
      'Run LanPilot on your computer, then scan its QR code or paste the pairing link.';

  @override
  String get scanQr => 'Scan QR code';

  @override
  String get scanHint => 'Point the camera at the QR code on your computer.';

  @override
  String get pasteLink => 'Paste pairing link';

  @override
  String get paste => 'Paste';

  @override
  String get pair => 'Pair';

  @override
  String get pairing => 'Pairing...';

  @override
  String get nearbyComputers => 'Nearby computers';

  @override
  String get searching => 'Searching...';

  @override
  String get manualIp => 'Enter IP address';

  @override
  String get address => 'Address (IP or IP:port)';

  @override
  String get password => 'Pairing password';

  @override
  String get pairWithPassword => 'Pair with password';

  @override
  String get errInvalidCode => 'This is not a LanPilot pairing link.';

  @override
  String get errUnreachable =>
      'Cannot reach the computer. Make sure both are on the same network.';

  @override
  String get errWrongComputer =>
      'A different computer answered than the one you are pairing with.';

  @override
  String get errCodeExpired =>
      'The pairing code expired. Refresh it on the computer and try again.';

  @override
  String get errWrongPassword => 'Wrong password.';

  @override
  String errLocked(int seconds) {
    return 'Too many attempts. Try again in $seconds s.';
  }

  @override
  String get errPasswordDisabled =>
      'Password pairing is turned off on this computer.';

  @override
  String get errDenied => 'The computer declined the pairing.';

  @override
  String errGeneric(String message) {
    return 'Something went wrong: $message';
  }

  @override
  String get actionFailed => 'That did not work. Check the connection.';

  @override
  String get comingSoon => 'Coming soon';

  @override
  String get mediaPlayPause => 'Play/Pause';

  @override
  String get mediaNext => 'Next';

  @override
  String get mediaPrevious => 'Previous';

  @override
  String get mediaVolumeUp => 'Volume up';

  @override
  String get mediaVolumeDown => 'Volume down';

  @override
  String get mediaMute => 'Mute';

  @override
  String get buttonLeft => 'Left';

  @override
  String get buttonRight => 'Right';

  @override
  String get sectionTouchpad => 'Touchpad';

  @override
  String get sensitivity => 'Sensitivity';

  @override
  String get acceleration => 'Acceleration';

  @override
  String get accelOff => 'Off';

  @override
  String get accelLow => 'Low';

  @override
  String get accelMedium => 'Medium';

  @override
  String get accelHigh => 'High';

  @override
  String get scrollSpeed => 'Scroll speed';

  @override
  String get naturalScroll => 'Natural scrolling';

  @override
  String get tapDelay => 'Tap delay';

  @override
  String tapDelayValue(int ms) {
    return '$ms ms';
  }

  @override
  String get haptics => 'Haptic feedback';

  @override
  String get showButtonBar => 'Show left and right buttons';

  @override
  String get sectionAppearance => 'Appearance';

  @override
  String get theme => 'Theme';

  @override
  String get themeNative => 'Native';

  @override
  String get themeDark => 'Dark';

  @override
  String get themeBrand => 'Brand';

  @override
  String get language => 'Language';

  @override
  String get languageSystem => 'System';

  @override
  String get languageZh => '中文';

  @override
  String get languageEn => 'English';

  @override
  String get sectionAbout => 'About';

  @override
  String get version => 'Version';

  @override
  String get osUnknown => 'Unknown';
}
