import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/app/version.dart';
import 'package:lanpilot/settings/settings.dart';
import 'package:lanpilot/settings/settings_page.dart';

import '../support/harness.dart';

void main() {
  testWidgets('switches apply immediately and persist', (tester) async {
    final h = await Harness.create();
    await tester.pumpWidget(testApp(h, const SettingsPage()));
    await tester.tap(find.byKey(const Key('setting-natural')));
    await tester.pump();
    expect(h.services.settings.value.naturalScroll, isFalse);
    await tester.ensureVisible(find.byKey(const Key('setting-buttons')));
    await tester.pump();
    await tester.tap(find.byKey(const Key('setting-buttons')));
    await tester.pump();
    expect(h.services.settings.value.showButtonBar, isFalse);
  });

  testWidgets('choosing a theme, acceleration and language', (tester) async {
    final h = await Harness.create();
    await tester.pumpWidget(testApp(h, const SettingsPage()));
    await tester.tap(find.text('High'));
    await tester.pump();
    expect(h.services.settings.value.accel, AccelPreset.high);
    await tester.scrollUntilVisible(
      find.byKey(const Key('setting-language')),
      200,
    );
    await tester.tap(find.text('Brand'));
    await tester.pump();
    expect(h.services.settings.value.theme, ThemeChoice.brand);
    await tester.tap(find.text('中文'));
    await tester.pump();
    expect(h.services.settings.value.language, LanguageChoice.zh);
  });

  testWidgets('sliders change their values', (tester) async {
    final h = await Harness.create();
    await tester.pumpWidget(testApp(h, const SettingsPage()));
    await tester.drag(
      find.byKey(const Key('setting-sensitivity')),
      const Offset(200, 0),
    );
    await tester.pump();
    expect(h.services.settings.value.sensitivity, greaterThan(1.0));
  });

  testWidgets('about shows the version', (tester) async {
    final h = await Harness.create();
    await tester.pumpWidget(testApp(h, const SettingsPage()));
    await tester.scrollUntilVisible(find.text(appVersion), 200);
    expect(find.text(appVersion), findsOneWidget);
  });
}
