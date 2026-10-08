import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../app/providers.dart';
import '../app/version.dart';
import '../l10n/app_localizations.dart';
import 'settings.dart';

class SettingsPage extends ConsumerWidget {
  const SettingsPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final controller = ref.watch(settingsProvider);
    final l = AppLocalizations.of(context);
    void set(Settings Function(Settings) change) =>
        unawaited(controller.update(change));

    return Scaffold(
      appBar: AppBar(title: Text(l.settings)),
      body: ValueListenableBuilder<Settings>(
        valueListenable: controller,
        builder: (context, s, _) => ListView(
          children: [
            _Section(l.sectionTouchpad),
            _SliderTile(
              key: const Key('setting-sensitivity'),
              label: l.sensitivity,
              value: s.sensitivity,
              min: Settings.minSensitivity,
              max: Settings.maxSensitivity,
              divisions: 15,
              display: '${s.sensitivity.toStringAsFixed(1)}x',
              onChanged: (v) => set((x) => x.copyWith(sensitivity: v)),
            ),
            ListTile(
              title: Text(l.acceleration),
              subtitle: Padding(
                padding: const EdgeInsets.only(top: 8),
                child: SegmentedButton<AccelPreset>(
                  key: const Key('setting-accel'),
                  showSelectedIcon: false,
                  segments: [
                    ButtonSegment(
                      value: AccelPreset.off,
                      label: Text(l.accelOff),
                    ),
                    ButtonSegment(
                      value: AccelPreset.low,
                      label: Text(l.accelLow),
                    ),
                    ButtonSegment(
                      value: AccelPreset.medium,
                      label: Text(l.accelMedium),
                    ),
                    ButtonSegment(
                      value: AccelPreset.high,
                      label: Text(l.accelHigh),
                    ),
                  ],
                  selected: {s.accel},
                  onSelectionChanged: (v) =>
                      set((x) => x.copyWith(accel: v.single)),
                ),
              ),
            ),
            _SliderTile(
              key: const Key('setting-scroll'),
              label: l.scrollSpeed,
              value: s.scrollSpeed,
              min: Settings.minScrollSpeed,
              max: Settings.maxScrollSpeed,
              divisions: 15,
              display: '${s.scrollSpeed.toStringAsFixed(1)}x',
              onChanged: (v) => set((x) => x.copyWith(scrollSpeed: v)),
            ),
            SwitchListTile(
              key: const Key('setting-natural'),
              title: Text(l.naturalScroll),
              value: s.naturalScroll,
              onChanged: (v) => set((x) => x.copyWith(naturalScroll: v)),
            ),
            _SliderTile(
              key: const Key('setting-tap-delay'),
              label: l.tapDelay,
              value: s.tapDelayMs.toDouble(),
              min: 0,
              max: Settings.maxTapDelayMs.toDouble(),
              divisions: 15,
              display: l.tapDelayValue(s.tapDelayMs),
              onChanged: (v) => set((x) => x.copyWith(tapDelayMs: v.round())),
            ),
            SwitchListTile(
              key: const Key('setting-haptics'),
              title: Text(l.haptics),
              value: s.haptics,
              onChanged: (v) => set((x) => x.copyWith(haptics: v)),
            ),
            SwitchListTile(
              key: const Key('setting-buttons'),
              title: Text(l.showButtonBar),
              value: s.showButtonBar,
              onChanged: (v) => set((x) => x.copyWith(showButtonBar: v)),
            ),
            _Section(l.sectionAppearance),
            ListTile(
              title: Text(l.theme),
              subtitle: Padding(
                padding: const EdgeInsets.only(top: 8),
                child: SegmentedButton<ThemeChoice>(
                  key: const Key('setting-theme'),
                  showSelectedIcon: false,
                  segments: [
                    ButtonSegment(
                      value: ThemeChoice.native,
                      label: Text(l.themeNative),
                    ),
                    ButtonSegment(
                      value: ThemeChoice.dark,
                      label: Text(l.themeDark),
                    ),
                    ButtonSegment(
                      value: ThemeChoice.brand,
                      label: Text(l.themeBrand),
                    ),
                  ],
                  selected: {s.theme},
                  onSelectionChanged: (v) =>
                      set((x) => x.copyWith(theme: v.single)),
                ),
              ),
            ),
            ListTile(
              title: Text(l.language),
              subtitle: Padding(
                padding: const EdgeInsets.only(top: 8),
                child: SegmentedButton<LanguageChoice>(
                  key: const Key('setting-language'),
                  showSelectedIcon: false,
                  segments: [
                    ButtonSegment(
                      value: LanguageChoice.system,
                      label: Text(l.languageSystem),
                    ),
                    ButtonSegment(
                      value: LanguageChoice.zh,
                      label: Text(l.languageZh),
                    ),
                    ButtonSegment(
                      value: LanguageChoice.en,
                      label: Text(l.languageEn),
                    ),
                  ],
                  selected: {s.language},
                  onSelectionChanged: (v) =>
                      set((x) => x.copyWith(language: v.single)),
                ),
              ),
            ),
            _Section(l.sectionAbout),
            ListTile(title: Text(l.version), trailing: const Text(appVersion)),
            const SizedBox(height: 24),
          ],
        ),
      ),
    );
  }
}

class _Section extends StatelessWidget {
  const _Section(this.title);

  final String title;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(16, 24, 16, 4),
    child: Text(
      title,
      style: Theme.of(context).textTheme.labelLarge
          ?.copyWith(color: Theme.of(context).colorScheme.primary),
    ),
  );
}

class _SliderTile extends StatelessWidget {
  const _SliderTile({
    super.key,
    required this.label,
    required this.value,
    required this.min,
    required this.max,
    required this.divisions,
    required this.display,
    required this.onChanged,
  });

  final String label;
  final double value;
  final double min;
  final double max;
  final int divisions;
  final String display;
  final ValueChanged<double> onChanged;

  @override
  Widget build(BuildContext context) => ListTile(
    title: Row(
      children: [
        Expanded(child: Text(label)),
        Text(display),
      ],
    ),
    subtitle: Slider(
      value: value.clamp(min, max),
      min: min,
      max: max,
      divisions: divisions,
      onChanged: onChanged,
    ),
  );
}
