import 'dart:async';
import 'dart:io';

import 'package:app_links/app_links.dart';
import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lanpilot_discovery/lanpilot_discovery.dart';
import 'package:path_provider/path_provider.dart';

import 'app/app.dart';
import 'app/bootstrap.dart';
import 'app/router.dart';
import 'bridge/generated/frb_generated.dart';
import 'bridge/lanpilot_client.dart';
import 'storage/json_store.dart';
import 'storage/secret_store.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final AppServices services;
  try {
    await RustLib.init();
    final docs = await getApplicationDocumentsDirectory();
    services = await createServices(
      client: RustLanPilotClient(),
      secrets: KeychainSecretStore(),
      openJson: (name) => JsonFile(File('${docs.path}/$name.json')),
      discoveryEvents: LanpilotDiscovery.events,
    );
  } on Object catch (e) {
    runApp(_StartupFailure('$e'));
    return;
  }
  final router = buildRouter(
    hasServers: services.servers.servers.value.isNotEmpty,
  );
  final network = Connectivity().onConnectivityChanged
      .map((results) => results.map((r) => r.name).join(','))
      .distinct()
      .map((_) {});
  runApp(
    ProviderScope(
      overrides: services.overrides,
      child: LanPilotApp(
        router: router,
        links: AppLinks().uriLinkStream,
        networkChanges: network,
      ),
    ),
  );
  unawaited(services.connection.start());
}

class _StartupFailure extends StatelessWidget {
  const _StartupFailure(this.error);

  final String error;

  @override
  Widget build(BuildContext context) => MaterialApp(
    home: Scaffold(
      body: Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Text(
                'LanPilot could not start',
                style: TextStyle(fontSize: 20, fontWeight: FontWeight.w600),
              ),
              const SizedBox(height: 12),
              Text(error, textAlign: TextAlign.center),
            ],
          ),
        ),
      ),
    ),
  );
}
