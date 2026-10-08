import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/media/media_page.dart';
import 'package:lanpilot/shortcuts/shortcuts_page.dart';

import '../support/fake_client.dart';
import '../support/harness.dart';

void main() {
  testWidgets('each button sends its media key', (tester) async {
    final h = await Harness.create(servers: [desk()]);
    await tester.pumpWidget(testApp(h, const Scaffold(body: MediaPage())));
    await h.connect(tester);
    for (final kind in MediaKind.values) {
      await tester.tap(find.byKey(Key('media-${kind.name}')));
      await tester.pump();
      expect(h.client.calls.last, 'media ${kind.name}');
    }
  });

  testWidgets('buttons are disabled while not connected', (tester) async {
    final h = await Harness.create(servers: [desk()]);
    await tester.pumpWidget(testApp(h, const Scaffold(body: MediaPage())));
    await tester.pump();
    await tester.tap(find.byKey(const Key('media-playPause')));
    await tester.pump();
    expect(h.client.calls.where((c) => c.startsWith('media')), isEmpty);
  });

  testWidgets('a failure is shown', (tester) async {
    final h = await Harness.create(servers: [desk()]);
    await tester.pumpWidget(testApp(h, const Scaffold(body: MediaPage())));
    await h.connect(tester);
    h.client.requestError = bridgeError(ErrorKind.closed);
    await tester.tap(find.byKey(const Key('media-mute')));
    await tester.pump();
    expect(
      find.text('That did not work. Check the connection.'),
      findsOneWidget,
    );
  });

  testWidgets('shortcuts say coming soon', (tester) async {
    final h = await Harness.create();
    await tester.pumpWidget(testApp(h, const Scaffold(body: ShortcutsPage())));
    expect(find.text('Coming soon'), findsOneWidget);
  });
}
