import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/settings/settings.dart';
import 'package:lanpilot/settings/settings_controller.dart';

import '../support/memory_stores.dart';

class _FailingStore extends MemoryJsonStore {
  @override
  Future<void> write(Map<String, Object?> data) =>
      Future.error(const FileSystemException('disk full'));
}

void main() {
  test('a failing write keeps the new value and does not throw', () async {
    final c = SettingsController(_FailingStore());
    await c.update((s) => s.copyWith(naturalScroll: false));
    expect(c.value.naturalScroll, isFalse);
  });

  test('defaults', () {
    const s = Settings();
    expect(s.sensitivity, 1.0);
    expect(s.accel, AccelPreset.medium);
    expect(s.scrollSpeed, 1.0);
    expect(s.naturalScroll, isTrue);
    expect(s.tapDelayMs, 180);
    expect(s.haptics, isTrue);
    expect(s.showButtonBar, isTrue);
    expect(s.theme, ThemeChoice.native);
    expect(s.language, LanguageChoice.system);
  });

  test('round trips through json', () {
    const s = Settings(
      sensitivity: 1.5,
      accel: AccelPreset.high,
      scrollSpeed: 0.5,
      naturalScroll: false,
      tapDelayMs: 100,
      haptics: false,
      showButtonBar: false,
      theme: ThemeChoice.brand,
      language: LanguageChoice.zh,
    );
    expect(Settings.fromJson(s.toJson()), s);
  });

  test('bad values fall back or are clamped', () {
    final s = Settings.fromJson({
      'sensitivity': 99,
      'accel': 'turbo',
      'scrollSpeed': 'fast',
      'tapDelayMs': -5,
      'theme': 'dark',
    });
    expect(s.sensitivity, Settings.maxSensitivity);
    expect(s.accel, AccelPreset.medium);
    expect(s.scrollSpeed, 1.0);
    expect(s.tapDelayMs, 0);
    expect(s.theme, ThemeChoice.dark);
  });

  test('controller persists updates', () async {
    final json = MemoryJsonStore();
    final c = SettingsController(json);
    await c.load();
    await c.update((s) => s.copyWith(haptics: false));
    final again = SettingsController(json);
    await again.load();
    expect(again.value.haptics, isFalse);
  });
}
