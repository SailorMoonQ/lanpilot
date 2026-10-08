import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:lanpilot/main.dart' as app;
import 'package:path_provider/path_provider.dart';

// Passed by tool/sim_e2e.sh.
const pairUri = String.fromEnvironment('PAIR_URI');
const agentId = String.fromEnvironment('AGENT_ID');

Future<void> pumpUntil(
  WidgetTester tester,
  Finder finder, {
  Duration timeout = const Duration(seconds: 20),
}) async {
  final end = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 100));
    if (finder.evaluate().isNotEmpty) return;
  }
  fail('timed out waiting for $finder');
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('pair by link, connect, move, click and play', (tester) async {
    expect(pairUri, isNotEmpty, reason: 'run this through tool/sim_e2e.sh');

    // Start from a fresh install state.
    final docs = await getApplicationDocumentsDirectory();
    for (final name in ['servers', 'settings']) {
      final f = File('${docs.path}/$name.json');
      if (f.existsSync()) f.deleteSync();
    }
    await const FlutterSecureStorage().deleteAll();

    await app.main();
    await pumpUntil(tester, find.byKey(const Key('paste-field')));

    // NWBrowser found the agent and core validated it.
    if (agentId.isNotEmpty) {
      // The app resolves a service for at most three tries of five seconds.
      await pumpUntil(
        tester,
        find.byKey(Key('nearby-$agentId')),
        timeout: const Duration(seconds: 30),
      );
    }

    await tester.enterText(find.byKey(const Key('paste-field')), pairUri);
    await tester.tap(find.byKey(const Key('paste-pair')));
    await pumpUntil(tester, find.byKey(const Key('status-connected')));

    await tester.timedDrag(
      find.byKey(const Key('touchpad')),
      const Offset(120, 40),
      const Duration(milliseconds: 400),
    );
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(find.byKey(const Key('touchpad')));
    await tester.pump(const Duration(milliseconds: 500));

    await tester.tap(find.byIcon(Icons.music_note_outlined));
    await pumpUntil(tester, find.byKey(const Key('media-playPause')));
    await tester.tap(find.byKey(const Key('media-playPause')));
    await tester.pump(const Duration(seconds: 1));
  });
}
