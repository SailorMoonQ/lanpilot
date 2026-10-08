import 'dart:ui';

import 'package:flutter/material.dart';

import '../settings/settings.dart';

/// Colors and materials of one theme (spec 2.1). The three themes share every
/// widget and differ only here.
@immutable
class LanPilotPalette extends ThemeExtension<LanPilotPalette> {
  const LanPilotPalette({
    required this.background,
    this.backgroundGradient,
    required this.touchpadFill,
    required this.touchpadBorder,
    required this.controlFill,
    required this.accent,
    required this.text,
    required this.secondaryText,
    required this.frosted,
    required this.touchpadInset,
  });

  final Color background;
  final Gradient? backgroundGradient;
  final Color touchpadFill;
  final Color touchpadBorder;
  final Color controlFill;
  final Color accent;
  final Color text;
  final Color secondaryText;

  /// Blur behind the touchpad and controls (brand theme).
  final bool frosted;

  /// Margin around the touchpad; smaller means a larger pad.
  final double touchpadInset;

  @override
  LanPilotPalette copyWith({Color? accent}) => LanPilotPalette(
    background: background,
    backgroundGradient: backgroundGradient,
    touchpadFill: touchpadFill,
    touchpadBorder: touchpadBorder,
    controlFill: controlFill,
    accent: accent ?? this.accent,
    text: text,
    secondaryText: secondaryText,
    frosted: frosted,
    touchpadInset: touchpadInset,
  );

  /// Themes switch instantly (spec 2.1), so no interpolation.
  @override
  LanPilotPalette lerp(LanPilotPalette? other, double t) =>
      t < 0.5 || other == null ? this : other;

  @override
  bool operator ==(Object other) =>
      other is LanPilotPalette &&
      other.background == background &&
      other.backgroundGradient == backgroundGradient &&
      other.touchpadFill == touchpadFill &&
      other.touchpadBorder == touchpadBorder &&
      other.controlFill == controlFill &&
      other.accent == accent &&
      other.text == text &&
      other.secondaryText == secondaryText &&
      other.frosted == frosted &&
      other.touchpadInset == touchpadInset;

  @override
  int get hashCode => Object.hash(
    background,
    backgroundGradient,
    touchpadFill,
    touchpadBorder,
    controlFill,
    accent,
    text,
    secondaryText,
    frosted,
    touchpadInset,
  );
}

const _white = Color(0xFFFFFFFF);

const _nativeLight = LanPilotPalette(
  background: Color(0xFFF2F2F7),
  touchpadFill: _white,
  touchpadBorder: Color(0x1F000000),
  controlFill: _white,
  accent: Color(0xFF007AFF),
  text: Color(0xFF000000),
  secondaryText: Color(0x993C3C43),
  frosted: false,
  touchpadInset: 16,
);

const _nativeDark = LanPilotPalette(
  background: Color(0xFF000000),
  touchpadFill: Color(0xFF1C1C1E),
  touchpadBorder: Color(0x33FFFFFF),
  controlFill: Color(0xFF1C1C1E),
  accent: Color(0xFF0A84FF),
  text: _white,
  secondaryText: Color(0x99EBEBF5),
  frosted: false,
  touchpadInset: 16,
);

const _darkImmersive = LanPilotPalette(
  background: Color(0xFF000000),
  touchpadFill: Color(0xFF0D0D0F),
  touchpadBorder: Color(0x14FFFFFF),
  controlFill: Color(0xFF1C1C1E),
  accent: Color(0xFF64D2FF),
  text: _white,
  secondaryText: Color(0x99EBEBF5),
  frosted: false,
  touchpadInset: 8,
);

const _brand = LanPilotPalette(
  background: Color(0xFF5B4BFF),
  backgroundGradient: LinearGradient(
    begin: Alignment.topLeft,
    end: Alignment.bottomRight,
    colors: [Color(0xFF5B4BFF), Color(0xFF8F5BFF), Color(0xFFFF7AA8)],
  ),
  touchpadFill: Color(0x33FFFFFF),
  touchpadBorder: Color(0x4DFFFFFF),
  controlFill: Color(0x33FFFFFF),
  accent: _white,
  text: _white,
  secondaryText: Color(0xCCFFFFFF),
  frosted: true,
  touchpadInset: 16,
);

/// Only the native theme follows the system; dark and brand look the same
/// in light and dark mode (spec 2.1).
Brightness effectiveBrightness(ThemeChoice choice, Brightness system) =>
    choice == ThemeChoice.native ? system : Brightness.dark;

LanPilotPalette paletteFor(ThemeChoice choice, Brightness brightness) =>
    switch (choice) {
      ThemeChoice.native =>
        brightness == Brightness.light ? _nativeLight : _nativeDark,
      ThemeChoice.dark => _darkImmersive,
      ThemeChoice.brand => _brand,
    };

ThemeData buildTheme(ThemeChoice choice, Brightness system) {
  final brightness = effectiveBrightness(choice, system);
  final p = paletteFor(choice, brightness);
  final base = ThemeData(brightness: brightness, useMaterial3: true);
  final scheme =
      ColorScheme.fromSeed(
        seedColor: p.accent == _white ? const Color(0xFF8F5BFF) : p.accent,
        brightness: brightness,
      ).copyWith(
        primary: p.accent,
        onPrimary: choice == ThemeChoice.brand
            ? const Color(0xFF5B4BFF)
            : _white,
        surface: p.controlFill,
        onSurface: p.text,
      );
  return base.copyWith(
    colorScheme: scheme,
    scaffoldBackgroundColor: p.backgroundGradient == null
        ? p.background
        : Colors.transparent,
    canvasColor: p.backgroundGradient == null
        ? p.background
        : const Color(0xFF6A50FF),
    textTheme: base.textTheme.apply(bodyColor: p.text, displayColor: p.text),
    iconTheme: IconThemeData(color: p.text),
    appBarTheme: AppBarTheme(
      backgroundColor: Colors.transparent,
      foregroundColor: p.text,
      elevation: 0,
      scrolledUnderElevation: 0,
    ),
    cardTheme: CardThemeData(
      color: p.controlFill,
      elevation: 0,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(14)),
    ),
    navigationBarTheme: NavigationBarThemeData(
      backgroundColor: p.backgroundGradient == null
          ? p.controlFill
          : Colors.transparent,
      indicatorColor: p.accent.withValues(alpha: 0.2),
    ),
    navigationRailTheme: const NavigationRailThemeData(
      backgroundColor: Colors.transparent,
    ),
    bottomSheetTheme: BottomSheetThemeData(
      backgroundColor: p.backgroundGradient == null
          ? p.controlFill
          : const Color(0xFF6A50FF),
    ),
    listTileTheme: ListTileThemeData(textColor: p.text, iconColor: p.text),
    extensions: [p],
  );
}

extension LanPilotThemeX on BuildContext {
  LanPilotPalette get palette => Theme.of(this).extension<LanPilotPalette>()!;
}

/// Paints the theme background (solid, or the brand gradient) behind a page.
class LanPilotBackground extends StatelessWidget {
  const LanPilotBackground({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final p = context.palette;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: p.background,
        gradient: p.backgroundGradient,
      ),
      child: child,
    );
  }
}

/// Frosted glass behind `child` when the theme asks for it.
class Frosted extends StatelessWidget {
  const Frosted({super.key, required this.child, required this.borderRadius});

  final Widget child;
  final BorderRadius borderRadius;

  @override
  Widget build(BuildContext context) {
    if (!context.palette.frosted) return child;
    return ClipRRect(
      borderRadius: borderRadius,
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 20, sigmaY: 20),
        child: child,
      ),
    );
  }
}
