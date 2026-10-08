@Tags(['golden'])
library;

import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/app/app.dart';
import 'package:lanpilot/app/router.dart';
import 'package:lanpilot/settings/settings.dart';
import 'package:lanpilot/touchpad/touchpad_page.dart';

import '../support/harness.dart';

const variants = [
  ('native-light', ThemeChoice.native, Brightness.light),
  ('native-dark', ThemeChoice.native, Brightness.dark),
  ('dark', ThemeChoice.dark, Brightness.dark),
  ('brand', ThemeChoice.brand, Brightness.light),
];

const languages = [('en', LanguageChoice.en), ('zh', LanguageChoice.zh)];

const pages = [
  ('control', '/control'),
  ('settings', '/settings'),
  ('add', '/add'),
];

void main() {
  for (final (page, location) in pages) {
    for (final (variant, theme, brightness) in variants) {
      for (final (lang, language) in languages) {
        testWidgets('$page $variant $lang', (tester) async {
          tester.view.physicalSize = const Size(1170, 2532);
          tester.view.devicePixelRatio = 3;
          tester.platformDispatcher.platformBrightnessTestValue = brightness;
          addTearDown(tester.view.reset);
          addTearDown(
            tester.platformDispatcher.clearPlatformBrightnessTestValue,
          );
          final original = keepAwakeHook;
          keepAwakeHook = (_) {};
          addTearDown(() => keepAwakeHook = original);

          final h = await Harness.create(
            servers: [desk()],
            settings: Settings(theme: theme, language: language),
          );
          await tester.pumpWidget(
            ProviderScope(
              overrides: h.services.overrides,
              child: LanPilotApp(
                router: buildRouter(
                  hasServers: true,
                  initialLocation: location,
                ),
              ),
            ),
          );
          await h.connect(tester);
          await tester.pump(const Duration(milliseconds: 500));
          await expectLater(
            find.byType(LanPilotApp),
            matchesGoldenFile('goldens/$page-$variant-$lang.png'),
          );
        }, skip: !Platform.isMacOS);
      }
    }
  }
}
