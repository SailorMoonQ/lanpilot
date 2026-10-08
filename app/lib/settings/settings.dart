import 'package:flutter/foundation.dart';

enum AccelPreset { off, low, medium, high }

enum ThemeChoice { native, dark, brand }

enum LanguageChoice { system, zh, en }

/// User settings (spec 1.1). Stored as settings.json.
@immutable
class Settings {
  const Settings({
    this.sensitivity = 1.0,
    this.accel = AccelPreset.medium,
    this.scrollSpeed = 1.0,
    this.naturalScroll = true,
    this.tapDelayMs = 180,
    this.haptics = true,
    this.showButtonBar = true,
    this.theme = ThemeChoice.native,
    this.language = LanguageChoice.system,
  });

  factory Settings.fromJson(Map<String, Object?> json) {
    const d = Settings();
    double number(String key, double fallback, double min, double max) {
      final v = json[key];
      return v is num ? v.toDouble().clamp(min, max) : fallback;
    }

    bool flag(String key, bool fallback) {
      final v = json[key];
      return v is bool ? v : fallback;
    }

    T choice<T extends Enum>(String key, List<T> values, T fallback) =>
        values.where((e) => e.name == json[key]).firstOrNull ?? fallback;

    final tap = json['tapDelayMs'];
    return Settings(
      sensitivity: number(
        'sensitivity',
        d.sensitivity,
        minSensitivity,
        maxSensitivity,
      ),
      accel: choice('accel', AccelPreset.values, d.accel),
      scrollSpeed: number(
        'scrollSpeed',
        d.scrollSpeed,
        minScrollSpeed,
        maxScrollSpeed,
      ),
      naturalScroll: flag('naturalScroll', d.naturalScroll),
      tapDelayMs: tap is num
          ? tap.round().clamp(0, maxTapDelayMs)
          : d.tapDelayMs,
      haptics: flag('haptics', d.haptics),
      showButtonBar: flag('showButtonBar', d.showButtonBar),
      theme: choice('theme', ThemeChoice.values, d.theme),
      language: choice('language', LanguageChoice.values, d.language),
    );
  }

  static const minSensitivity = 0.5;
  static const maxSensitivity = 2.0;
  static const minScrollSpeed = 0.5;
  static const maxScrollSpeed = 2.0;
  static const maxTapDelayMs = 300;

  final double sensitivity;
  final AccelPreset accel;
  final double scrollSpeed;
  final bool naturalScroll;
  final int tapDelayMs;
  final bool haptics;
  final bool showButtonBar;
  final ThemeChoice theme;
  final LanguageChoice language;

  Settings copyWith({
    double? sensitivity,
    AccelPreset? accel,
    double? scrollSpeed,
    bool? naturalScroll,
    int? tapDelayMs,
    bool? haptics,
    bool? showButtonBar,
    ThemeChoice? theme,
    LanguageChoice? language,
  }) => Settings(
    sensitivity: sensitivity ?? this.sensitivity,
    accel: accel ?? this.accel,
    scrollSpeed: scrollSpeed ?? this.scrollSpeed,
    naturalScroll: naturalScroll ?? this.naturalScroll,
    tapDelayMs: tapDelayMs ?? this.tapDelayMs,
    haptics: haptics ?? this.haptics,
    showButtonBar: showButtonBar ?? this.showButtonBar,
    theme: theme ?? this.theme,
    language: language ?? this.language,
  );

  Map<String, Object?> toJson() => {
    'sensitivity': sensitivity,
    'accel': accel.name,
    'scrollSpeed': scrollSpeed,
    'naturalScroll': naturalScroll,
    'tapDelayMs': tapDelayMs,
    'haptics': haptics,
    'showButtonBar': showButtonBar,
    'theme': theme.name,
    'language': language.name,
  };

  @override
  bool operator ==(Object other) =>
      other is Settings &&
      other.sensitivity == sensitivity &&
      other.accel == accel &&
      other.scrollSpeed == scrollSpeed &&
      other.naturalScroll == naturalScroll &&
      other.tapDelayMs == tapDelayMs &&
      other.haptics == haptics &&
      other.showButtonBar == showButtonBar &&
      other.theme == theme &&
      other.language == language;

  @override
  int get hashCode => Object.hash(
    sensitivity,
    accel,
    scrollSpeed,
    naturalScroll,
    tapDelayMs,
    haptics,
    showButtonBar,
    theme,
    language,
  );
}
