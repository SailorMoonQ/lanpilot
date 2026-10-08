import 'package:flutter_riverpod/misc.dart' show Override;
import 'package:lanpilot_discovery/lanpilot_discovery.dart';

import '../bridge/lanpilot_client.dart';
import '../connection/connection_manager.dart';
import '../connection/server_store.dart';
import '../discovery/discovery_service.dart';
import '../settings/settings_controller.dart';
import '../storage/identity_store.dart';
import '../storage/json_store.dart';
import '../storage/secret_store.dart';
import 'providers.dart';
import 'version.dart';

class AppServices {
  AppServices({
    required this.client,
    required this.servers,
    required this.settings,
    required this.discovery,
    required this.connection,
  });

  final LanPilotClient client;
  final ServerStore servers;
  final SettingsController settings;
  final DiscoveryService discovery;
  final ConnectionManager connection;

  List<Override> get overrides => [
    clientProvider.overrideWithValue(client),
    serverStoreProvider.overrideWithValue(servers),
    settingsProvider.overrideWithValue(settings),
    discoveryProvider.overrideWithValue(discovery),
    connectionProvider.overrideWithValue(connection),
  ];
}

/// Builds every service. Does not connect: call `connection.start()` once the
/// UI is up.
Future<AppServices> createServices({
  required LanPilotClient client,
  required SecretStore secrets,
  required JsonStore Function(String name) openJson,
  required Stream<RawService> Function() discoveryEvents,
  String deviceName = 'iPhone',
  DateTime Function()? now,
}) async {
  final secret = await IdentityStore(
    secrets,
    client.generateSecret,
  ).loadOrCreate();
  client.initClient(
    secret: secret,
    deviceName: deviceName,
    appVersion: appVersion,
  );
  final servers = ServerStore(openJson('servers'), secrets, now: now);
  await servers.load();
  final settings = SettingsController(openJson('settings'));
  await settings.load();
  final discovery = DiscoveryService(
    discoveryEvents,
    (raw) => client.validateDiscovered(
      fullname: raw.fullname,
      txt: raw.txt,
      addrs: raw.addrs,
      port: raw.port,
    ),
  )..start();
  final connection = ConnectionManager(
    client: client,
    store: servers,
    discovery: discovery,
  );
  return AppServices(
    client: client,
    servers: servers,
    settings: settings,
    discovery: discovery,
    connection: connection,
  );
}
