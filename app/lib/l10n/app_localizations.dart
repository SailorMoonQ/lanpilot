import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_en.dart';
import 'app_localizations_zh.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'l10n/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations)!;
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[
    Locale('en'),
    Locale('zh'),
  ];

  /// No description provided for @appTitle.
  ///
  /// In en, this message translates to:
  /// **'LanPilot'**
  String get appTitle;

  /// No description provided for @tabTouchpad.
  ///
  /// In en, this message translates to:
  /// **'Touchpad'**
  String get tabTouchpad;

  /// No description provided for @tabShortcuts.
  ///
  /// In en, this message translates to:
  /// **'Shortcuts'**
  String get tabShortcuts;

  /// No description provided for @tabMedia.
  ///
  /// In en, this message translates to:
  /// **'Media'**
  String get tabMedia;

  /// No description provided for @statusConnecting.
  ///
  /// In en, this message translates to:
  /// **'Connecting...'**
  String get statusConnecting;

  /// No description provided for @statusReconnecting.
  ///
  /// In en, this message translates to:
  /// **'Reconnecting...'**
  String get statusReconnecting;

  /// No description provided for @statusOffline.
  ///
  /// In en, this message translates to:
  /// **'The computer is offline. Retrying...'**
  String get statusOffline;

  /// No description provided for @statusRemoved.
  ///
  /// In en, this message translates to:
  /// **'This computer removed this device. Pair again.'**
  String get statusRemoved;

  /// No description provided for @statusUpdateComputer.
  ///
  /// In en, this message translates to:
  /// **'Please update LanPilot on the computer.'**
  String get statusUpdateComputer;

  /// No description provided for @statusUpdateApp.
  ///
  /// In en, this message translates to:
  /// **'Please update the app.'**
  String get statusUpdateApp;

  /// No description provided for @pairAgain.
  ///
  /// In en, this message translates to:
  /// **'Pair again'**
  String get pairAgain;

  /// No description provided for @localNetworkTitle.
  ///
  /// In en, this message translates to:
  /// **'Local network access needed'**
  String get localNetworkTitle;

  /// No description provided for @localNetworkBody.
  ///
  /// In en, this message translates to:
  /// **'Turn on LanPilot in Settings > Privacy & Security > Local Network.'**
  String get localNetworkBody;

  /// No description provided for @openSettings.
  ///
  /// In en, this message translates to:
  /// **'Open Settings'**
  String get openSettings;

  /// No description provided for @retry.
  ///
  /// In en, this message translates to:
  /// **'Retry'**
  String get retry;

  /// No description provided for @switcherTitle.
  ///
  /// In en, this message translates to:
  /// **'Computers'**
  String get switcherTitle;

  /// No description provided for @online.
  ///
  /// In en, this message translates to:
  /// **'Online'**
  String get online;

  /// No description provided for @offline.
  ///
  /// In en, this message translates to:
  /// **'Offline'**
  String get offline;

  /// No description provided for @unpair.
  ///
  /// In en, this message translates to:
  /// **'Unpair'**
  String get unpair;

  /// No description provided for @unpairConfirm.
  ///
  /// In en, this message translates to:
  /// **'Unpair \"{name}\"?'**
  String unpairConfirm(String name);

  /// No description provided for @cancel.
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get cancel;

  /// No description provided for @addComputer.
  ///
  /// In en, this message translates to:
  /// **'Add computer'**
  String get addComputer;

  /// No description provided for @settings.
  ///
  /// In en, this message translates to:
  /// **'Settings'**
  String get settings;

  /// No description provided for @addComputerHint.
  ///
  /// In en, this message translates to:
  /// **'Run LanPilot on your computer, then scan its QR code or paste the pairing link.'**
  String get addComputerHint;

  /// No description provided for @scanQr.
  ///
  /// In en, this message translates to:
  /// **'Scan QR code'**
  String get scanQr;

  /// No description provided for @scanHint.
  ///
  /// In en, this message translates to:
  /// **'Point the camera at the QR code on your computer.'**
  String get scanHint;

  /// No description provided for @pasteLink.
  ///
  /// In en, this message translates to:
  /// **'Paste pairing link'**
  String get pasteLink;

  /// No description provided for @paste.
  ///
  /// In en, this message translates to:
  /// **'Paste'**
  String get paste;

  /// No description provided for @pair.
  ///
  /// In en, this message translates to:
  /// **'Pair'**
  String get pair;

  /// No description provided for @pairing.
  ///
  /// In en, this message translates to:
  /// **'Pairing...'**
  String get pairing;

  /// No description provided for @nearbyComputers.
  ///
  /// In en, this message translates to:
  /// **'Nearby computers'**
  String get nearbyComputers;

  /// No description provided for @searching.
  ///
  /// In en, this message translates to:
  /// **'Searching...'**
  String get searching;

  /// No description provided for @manualIp.
  ///
  /// In en, this message translates to:
  /// **'Enter IP address'**
  String get manualIp;

  /// No description provided for @address.
  ///
  /// In en, this message translates to:
  /// **'Address (IP or IP:port)'**
  String get address;

  /// No description provided for @password.
  ///
  /// In en, this message translates to:
  /// **'Pairing password'**
  String get password;

  /// No description provided for @pairWithPassword.
  ///
  /// In en, this message translates to:
  /// **'Pair with password'**
  String get pairWithPassword;

  /// No description provided for @errInvalidCode.
  ///
  /// In en, this message translates to:
  /// **'This is not a LanPilot pairing link.'**
  String get errInvalidCode;

  /// No description provided for @errUnreachable.
  ///
  /// In en, this message translates to:
  /// **'Cannot reach the computer. Make sure both are on the same network.'**
  String get errUnreachable;

  /// No description provided for @errWrongComputer.
  ///
  /// In en, this message translates to:
  /// **'A different computer answered than the one you are pairing with.'**
  String get errWrongComputer;

  /// No description provided for @errCodeExpired.
  ///
  /// In en, this message translates to:
  /// **'The pairing code expired. Refresh it on the computer and try again.'**
  String get errCodeExpired;

  /// No description provided for @errWrongPassword.
  ///
  /// In en, this message translates to:
  /// **'Wrong password.'**
  String get errWrongPassword;

  /// No description provided for @errLocked.
  ///
  /// In en, this message translates to:
  /// **'Too many attempts. Try again in {seconds} s.'**
  String errLocked(int seconds);

  /// No description provided for @errPasswordDisabled.
  ///
  /// In en, this message translates to:
  /// **'Password pairing is turned off on this computer.'**
  String get errPasswordDisabled;

  /// No description provided for @errDenied.
  ///
  /// In en, this message translates to:
  /// **'The computer declined the pairing.'**
  String get errDenied;

  /// No description provided for @errGeneric.
  ///
  /// In en, this message translates to:
  /// **'Something went wrong: {message}'**
  String errGeneric(String message);

  /// No description provided for @actionFailed.
  ///
  /// In en, this message translates to:
  /// **'That did not work. Check the connection.'**
  String get actionFailed;

  /// No description provided for @comingSoon.
  ///
  /// In en, this message translates to:
  /// **'Coming soon'**
  String get comingSoon;

  /// No description provided for @mediaPlayPause.
  ///
  /// In en, this message translates to:
  /// **'Play/Pause'**
  String get mediaPlayPause;

  /// No description provided for @mediaNext.
  ///
  /// In en, this message translates to:
  /// **'Next'**
  String get mediaNext;

  /// No description provided for @mediaPrevious.
  ///
  /// In en, this message translates to:
  /// **'Previous'**
  String get mediaPrevious;

  /// No description provided for @mediaVolumeUp.
  ///
  /// In en, this message translates to:
  /// **'Volume up'**
  String get mediaVolumeUp;

  /// No description provided for @mediaVolumeDown.
  ///
  /// In en, this message translates to:
  /// **'Volume down'**
  String get mediaVolumeDown;

  /// No description provided for @mediaMute.
  ///
  /// In en, this message translates to:
  /// **'Mute'**
  String get mediaMute;

  /// No description provided for @buttonLeft.
  ///
  /// In en, this message translates to:
  /// **'Left'**
  String get buttonLeft;

  /// No description provided for @buttonRight.
  ///
  /// In en, this message translates to:
  /// **'Right'**
  String get buttonRight;

  /// No description provided for @sectionTouchpad.
  ///
  /// In en, this message translates to:
  /// **'Touchpad'**
  String get sectionTouchpad;

  /// No description provided for @sensitivity.
  ///
  /// In en, this message translates to:
  /// **'Sensitivity'**
  String get sensitivity;

  /// No description provided for @acceleration.
  ///
  /// In en, this message translates to:
  /// **'Acceleration'**
  String get acceleration;

  /// No description provided for @accelOff.
  ///
  /// In en, this message translates to:
  /// **'Off'**
  String get accelOff;

  /// No description provided for @accelLow.
  ///
  /// In en, this message translates to:
  /// **'Low'**
  String get accelLow;

  /// No description provided for @accelMedium.
  ///
  /// In en, this message translates to:
  /// **'Medium'**
  String get accelMedium;

  /// No description provided for @accelHigh.
  ///
  /// In en, this message translates to:
  /// **'High'**
  String get accelHigh;

  /// No description provided for @scrollSpeed.
  ///
  /// In en, this message translates to:
  /// **'Scroll speed'**
  String get scrollSpeed;

  /// No description provided for @naturalScroll.
  ///
  /// In en, this message translates to:
  /// **'Natural scrolling'**
  String get naturalScroll;

  /// No description provided for @tapDelay.
  ///
  /// In en, this message translates to:
  /// **'Tap delay'**
  String get tapDelay;

  /// No description provided for @tapDelayValue.
  ///
  /// In en, this message translates to:
  /// **'{ms} ms'**
  String tapDelayValue(int ms);

  /// No description provided for @haptics.
  ///
  /// In en, this message translates to:
  /// **'Haptic feedback'**
  String get haptics;

  /// No description provided for @showButtonBar.
  ///
  /// In en, this message translates to:
  /// **'Show left and right buttons'**
  String get showButtonBar;

  /// No description provided for @sectionAppearance.
  ///
  /// In en, this message translates to:
  /// **'Appearance'**
  String get sectionAppearance;

  /// No description provided for @theme.
  ///
  /// In en, this message translates to:
  /// **'Theme'**
  String get theme;

  /// No description provided for @themeNative.
  ///
  /// In en, this message translates to:
  /// **'Native'**
  String get themeNative;

  /// No description provided for @themeDark.
  ///
  /// In en, this message translates to:
  /// **'Dark'**
  String get themeDark;

  /// No description provided for @themeBrand.
  ///
  /// In en, this message translates to:
  /// **'Brand'**
  String get themeBrand;

  /// No description provided for @language.
  ///
  /// In en, this message translates to:
  /// **'Language'**
  String get language;

  /// No description provided for @languageSystem.
  ///
  /// In en, this message translates to:
  /// **'System'**
  String get languageSystem;

  /// No description provided for @languageZh.
  ///
  /// In en, this message translates to:
  /// **'中文'**
  String get languageZh;

  /// No description provided for @languageEn.
  ///
  /// In en, this message translates to:
  /// **'English'**
  String get languageEn;

  /// No description provided for @sectionAbout.
  ///
  /// In en, this message translates to:
  /// **'About'**
  String get sectionAbout;

  /// No description provided for @version.
  ///
  /// In en, this message translates to:
  /// **'Version'**
  String get version;

  /// No description provided for @osUnknown.
  ///
  /// In en, this message translates to:
  /// **'Unknown'**
  String get osUnknown;
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['en', 'zh'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return AppLocalizationsEn();
    case 'zh':
      return AppLocalizationsZh();
  }

  throw FlutterError(
    'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
