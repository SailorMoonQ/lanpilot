import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../l10n/app_localizations.dart';
import '../pairing/pairing_controller.dart';
import '../settings/settings.dart';
import '../theme/app_theme.dart';
import 'providers.dart';

Locale? localeFor(LanguageChoice choice) => switch (choice) {
  LanguageChoice.system => null,
  LanguageChoice.zh => const Locale('zh'),
  LanguageChoice.en => const Locale('en'),
};

class LanPilotApp extends ConsumerStatefulWidget {
  const LanPilotApp({
    super.key,
    required this.router,
    this.links = const Stream.empty(),
    this.networkChanges = const Stream.empty(),
  });

  final GoRouter router;

  /// `lanpilot://` links opened from the system camera or other apps.
  final Stream<Uri> links;
  final Stream<void> networkChanges;

  @override
  ConsumerState<LanPilotApp> createState() => _LanPilotAppState();
}

class _LanPilotAppState extends ConsumerState<LanPilotApp> {
  final _messenger = GlobalKey<ScaffoldMessengerState>();
  late final AppLifecycleListener _lifecycle;
  late final StreamSubscription<Uri> _links;
  late final StreamSubscription<void> _network;

  @override
  void initState() {
    super.initState();
    final connection = ref.read(connectionProvider);
    _lifecycle = AppLifecycleListener(
      onPause: () => unawaited(connection.onPaused()),
      onResume: () => unawaited(connection.onResumed()),
    );
    _network = widget.networkChanges.listen(
      (_) => unawaited(connection.onNetworkChanged()),
    );
    _links = widget.links.listen((uri) => unawaited(_onLink(uri)));
  }

  @override
  void dispose() {
    _lifecycle.dispose();
    unawaited(_links.cancel());
    unawaited(_network.cancel());
    super.dispose();
  }

  Future<void> _onLink(Uri uri) async {
    if (uri.scheme != 'lanpilot') return;
    try {
      await ref.read(pairingProvider).pairWithUri(uri.toString());
      widget.router.go('/control');
    } on Object catch (e) {
      final context = widget.router.routerDelegate.navigatorKey.currentContext;
      if (context == null || !context.mounted) return;
      _messenger.currentState?.showSnackBar(
        SnackBar(
          content: Text(pairingFailureText(e, AppLocalizations.of(context))),
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final settings = ref.watch(settingsProvider);
    return ValueListenableBuilder<Settings>(
      valueListenable: settings,
      builder: (context, s, _) => MaterialApp.router(
        title: 'LanPilot',
        debugShowCheckedModeBanner: false,
        scaffoldMessengerKey: _messenger,
        routerConfig: widget.router,
        theme: buildTheme(s.theme, Brightness.light),
        darkTheme: buildTheme(s.theme, Brightness.dark),
        themeMode: ThemeMode.system,
        locale: localeFor(s.language),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        builder: (context, child) =>
            LanPilotBackground(child: child ?? const SizedBox()),
      ),
    );
  }
}
