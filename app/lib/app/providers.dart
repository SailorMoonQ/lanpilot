import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/lanpilot_client.dart';
import '../connection/connection_manager.dart';
import '../connection/server_store.dart';
import '../discovery/discovery_service.dart';
import '../settings/settings_controller.dart';

// Service locator: real instances come from AppServices.overrides (main.dart)
// and fakes from the test harness.
Never _missing(String name) =>
    throw StateError('$name must be overridden with AppServices.overrides');

final clientProvider = Provider<LanPilotClient>(
  (ref) => _missing('clientProvider'),
);
final serverStoreProvider = Provider<ServerStore>(
  (ref) => _missing('serverStoreProvider'),
);
final settingsProvider = Provider<SettingsController>(
  (ref) => _missing('settingsProvider'),
);
final discoveryProvider = Provider<DiscoveryService>(
  (ref) => _missing('discoveryProvider'),
);
final connectionProvider = Provider<ConnectionManager>(
  (ref) => _missing('connectionProvider'),
);
