import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/settings/settings.dart';
import 'package:lanpilot/theme/app_theme.dart';

void main() {
  test('native follows the system brightness', () {
    expect(
      buildTheme(ThemeChoice.native, Brightness.light).brightness,
      Brightness.light,
    );
    expect(
      buildTheme(ThemeChoice.native, Brightness.dark).brightness,
      Brightness.dark,
    );
    expect(
      paletteFor(ThemeChoice.native, Brightness.light).background,
      const Color(0xFFF2F2F7),
    );
    expect(
      paletteFor(ThemeChoice.native, Brightness.dark).background,
      const Color(0xFF000000),
    );
  });

  test('dark and brand ignore the system brightness', () {
    for (final choice in [ThemeChoice.dark, ThemeChoice.brand]) {
      expect(buildTheme(choice, Brightness.light).brightness, Brightness.dark);
      expect(
        paletteFor(choice, effectiveBrightness(choice, Brightness.light)),
        paletteFor(choice, effectiveBrightness(choice, Brightness.dark)),
      );
    }
  });

  test('palettes match the spec', () {
    final dark = paletteFor(ThemeChoice.dark, Brightness.dark);
    expect(dark.background, const Color(0xFF000000));
    expect(dark.touchpadFill, const Color(0xFF0D0D0F));
    expect(dark.accent, const Color(0xFF64D2FF));
    expect(
      dark.touchpadInset,
      lessThan(paletteFor(ThemeChoice.native, Brightness.light).touchpadInset),
    );

    final brand = paletteFor(ThemeChoice.brand, Brightness.dark);
    expect(brand.frosted, isTrue);
    final gradient = brand.backgroundGradient! as LinearGradient;
    expect(gradient.colors, const [
      Color(0xFF5B4BFF),
      Color(0xFF8F5BFF),
      Color(0xFFFF7AA8),
    ]);
    expect(brand.accent, const Color(0xFFFFFFFF));
  });

  testWidgets('the palette is reachable from context', (tester) async {
    late LanPilotPalette palette;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(ThemeChoice.brand, Brightness.light),
        home: Builder(
          builder: (context) {
            palette = context.palette;
            return const LanPilotBackground(child: SizedBox());
          },
        ),
      ),
    );
    expect(palette.frosted, isTrue);
  });
  test('dark and native onPrimary contrast the accent', () {
    expect(
      buildTheme(ThemeChoice.dark, Brightness.dark).colorScheme.onPrimary,
      const Color(0xFF000000),
    );
    expect(
      buildTheme(ThemeChoice.native, Brightness.light).colorScheme.onPrimary,
      const Color(0xFFFFFFFF),
    );
  });
}
