import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/app/app.dart';
import 'package:lanpilot/app/router.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/settings/settings.dart';

import '../support/harness.dart';

Future<void> pumpApp(
  WidgetTester tester,
  Harness h, {
  Stream<Uri> links = const Stream.empty(),
  Stream<void> network = const Stream.empty(),
}) async {
  await tester.pumpWidget(
    ProviderScope(
      overrides: h.services.overrides,
      child: LanPilotApp(
        router: buildRouter(
          hasServers: h.services.servers.servers.value.isNotEmpty,
        ),
        links: links,
        networkChanges: network,
      ),
    ),
  );
  await tester.pump();
}

int connects(Harness h) =>
    h.client.calls.where((c) => c.startsWith('connect')).length;

void main() {
  testWidgets('no paired computer opens Add computer', (tester) async {
    final h = await Harness.create();
    await pumpApp(tester, h);
    expect(find.byKey(const Key('paste-field')), findsOneWidget);
  });

  testWidgets('a paired computer opens the touchpad and connects', (
    tester,
  ) async {
    final h = await Harness.create(servers: [desk()]);
    await pumpApp(tester, h);
    await h.connect(tester);
    expect(find.byKey(const Key('touchpad')), findsOneWidget);
    expect(find.byKey(const Key('status-connected')), findsOneWidget);
  });

  testWidgets('tabs switch pages', (tester) async {
    // The default test view is landscape, where the rail hides its labels.
    tester.view.physicalSize = const Size(1170, 2532);
    tester.view.devicePixelRatio = 3;
    addTearDown(tester.view.reset);
    final h = await Harness.create(servers: [desk()]);
    await pumpApp(tester, h);
    await h.connect(tester);
    await tester.tap(find.text('Media'));
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('media-playPause')), findsOneWidget);
    await tester.tap(find.text('Shortcuts'));
    await tester.pumpAndSettle();
    expect(find.text('Coming soon'), findsOneWidget);
  });

  testWidgets('landscape uses a side rail', (tester) async {
    tester.view.physicalSize = const Size(2532, 1170);
    tester.view.devicePixelRatio = 3;
    addTearDown(tester.view.reset);
    final h = await Harness.create(servers: [desk()]);
    await pumpApp(tester, h);
    expect(find.byType(NavigationRail), findsOneWidget);
    expect(find.byType(NavigationBar), findsNothing);
  });

  testWidgets('background disconnects, foreground reconnects', (tester) async {
    final h = await Harness.create(servers: [desk()]);
    await pumpApp(tester, h);
    await h.connect(tester);
    final binding = tester.binding;
    for (final s in [
      AppLifecycleState.inactive,
      AppLifecycleState.hidden,
      AppLifecycleState.paused,
    ]) {
      binding.handleAppLifecycleStateChanged(s);
    }
    await tester.pump();
    expect(h.client.calls, contains('disconnect'));
    for (final s in [
      AppLifecycleState.hidden,
      AppLifecycleState.inactive,
      AppLifecycleState.resumed,
    ]) {
      binding.handleAppLifecycleStateChanged(s);
    }
    await tester.pump(const Duration(seconds: 2));
    expect(connects(h), 2);
    expect(find.byKey(const Key('status-connected')), findsOneWidget);
  });

  testWidgets('a lanpilot link pairs from anywhere', (tester) async {
    final links = StreamController<Uri>();
    final h = await Harness.create();
    await pumpApp(tester, h, links: links.stream);
    links.add(Uri.parse('lanpilot://pair?d=abc'));
    await tester.pump(const Duration(seconds: 2));
    await tester.pumpAndSettle();
    expect(h.client.calls, contains('pairWithUri lanpilot://pair?d=abc'));
    expect(find.byKey(const Key('touchpad')), findsOneWidget);
  });

  testWidgets('a network change reconnects', (tester) async {
    final network = StreamController<void>();
    final h = await Harness.create(servers: [desk()]);
    await pumpApp(tester, h, network: network.stream);
    await h.connect(tester);
    network.add(null);
    await tester.pump(const Duration(seconds: 2));
    expect(connects(h), 2);
  });

  testWidgets('a slow start opens the switcher', (tester) async {
    final h = await Harness.create(servers: [desk()]);
    h.client.connectResults.add(Completer<SessionInfo>());
    await pumpApp(tester, h);
    unawaited(h.connection.start());
    await tester.pump(const Duration(seconds: 6));
    // A spinner is running, so no pumpAndSettle: just let the sheet animate.
    await tester.pump(const Duration(seconds: 1));
    expect(find.byKey(const Key('switcher-add')), findsOneWidget);
  });

  testWidgets('the language setting switches to Chinese', (tester) async {
    final h = await Harness.create(
      servers: [desk()],
      settings: const Settings(language: LanguageChoice.zh),
    );
    await pumpApp(tester, h);
    expect(find.text('触摸板'), findsOneWidget);
  });

  testWidgets('losing the last computer returns to Add computer', (
    tester,
  ) async {
    final h = await Harness.create(servers: [desk()]);
    await pumpApp(tester, h);
    await h.connect(tester);
    h.client.closeSession(1, CloseReason.unpaired);
    await tester.pump();
    // "Searching..." spins on the add page, so no pumpAndSettle.
    await tester.pump(const Duration(seconds: 1));
    expect(find.byKey(const Key('paste-field')), findsOneWidget);
  });
}
