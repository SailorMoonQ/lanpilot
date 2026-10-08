import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:lanpilot/app/bootstrap.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/connection/connection_manager.dart';
import 'package:lanpilot/connection/paired_server.dart';
import 'package:lanpilot/l10n/app_localizations.dart';
import 'package:lanpilot/settings/settings.dart';
import 'package:lanpilot/theme/app_theme.dart';
import 'package:lanpilot_discovery/lanpilot_discovery.dart';

import 'fake_client.dart';
import 'memory_stores.dart';

const deskId = '0123456789abcdef';

PairedServer desk({String shortId = deskId, String name = 'Desk'}) =>
    PairedServer(
      shortId: shortId,
      publicKeyHex: 'ab' * 32,
      name: name,
      os: OsKind.windows,
      addrs: const ['192.168.1.10:45810'],
    );

/// Real services over fakes: the fake bridge, memory stores and a fake
/// Bonjour stream. Everything completes in microtasks, so it works inside
/// testWidgets' fake time.
class Harness {
  Harness._(this.client, this.discoveryEvents, this.services, this._stores);

  static Future<Harness> create({
    List<PairedServer> servers = const [],
    Settings settings = const Settings(),
    bool everConnected = true,
  }) async {
    final client = FakeLanPilotClient();
    final events = StreamController<RawService>.broadcast();
    final stores = <String, MemoryJsonStore>{
      'servers': MemoryJsonStore('{"everConnected": $everConnected}'),
    };
    var clock = DateTime.utc(2026, 10, 8);
    final services = await createServices(
      client: client,
      secrets: MemorySecretStore(),
      openJson: (name) => stores.putIfAbsent(name, MemoryJsonStore.new),
      discoveryEvents: () => events.stream,
      now: () => clock = clock.add(const Duration(seconds: 1)),
    );
    for (final s in servers.reversed) {
      await services.servers.upsert(s);
      await services.servers.markUsed(s.shortId);
    }
    await services.settings.update((_) => settings);
    return Harness._(client, events, services, stores);
  }

  final FakeLanPilotClient client;
  final StreamController<RawService> discoveryEvents;
  final AppServices services;
  final Map<String, MemoryJsonStore> _stores;

  /// The in-memory file behind `name` (for example 'settings').
  MemoryJsonStore json(String name) => _stores[name]!;

  ConnectionManager get connection => services.connection;

  /// Starts the connection manager and lets it connect.
  Future<void> connect(WidgetTester tester) async {
    unawaited(connection.start());
    await tester.pump(const Duration(seconds: 2));
  }

  /// Announces a computer on the fake Bonjour stream.
  void announce(DiscoveredInfo info) {
    final fullname = '${info.shortId}._lanpilot._udp.local.';
    client.discoverable[fullname] = info;
    discoveryEvents.add(
      RawService(
        found: true,
        fullname: fullname,
        addrs: const ['10.0.0.2'],
        port: 45810,
      ),
    );
  }
}

const stubRoutes = [
  '/add',
  '/add/scan',
  '/add/manual',
  '/add/nearby',
  '/settings',
  '/control',
];

Widget testApp(
  Harness h,
  Widget home, {
  Locale locale = const Locale('en'),
  ThemeChoice theme = ThemeChoice.native,
  Brightness brightness = Brightness.light,
}) {
  final router = GoRouter(
    initialLocation: '/test',
    routes: [
      GoRoute(path: '/test', builder: (_, _) => home),
      for (final path in stubRoutes)
        GoRoute(
          path: path,
          builder: (_, _) => Scaffold(body: Text('route:$path')),
        ),
    ],
  );
  return ProviderScope(
    overrides: h.services.overrides,
    child: MaterialApp.router(
      routerConfig: router,
      locale: locale,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      theme: buildTheme(theme, brightness),
      builder: (context, child) => LanPilotBackground(child: child!),
    ),
  );
}
