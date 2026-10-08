import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/connection/conn_state.dart';
import 'package:lanpilot/connection/connection_overlay.dart';
import 'package:lanpilot/connection/status_header.dart';

import '../support/fake_discovery.dart';
import '../support/harness.dart';

const otherId = 'fedcba9876543210';

Widget header() => const Scaffold(body: SafeArea(child: StatusHeader()));

void main() {
  testWidgets('the header shows the computer and its status', (tester) async {
    final h = await Harness.create(servers: [desk()]);
    await tester.pumpWidget(testApp(h, header()));
    await h.connect(tester);
    expect(find.text('Desk'), findsOneWidget);
    expect(find.byKey(const Key('status-connected')), findsOneWidget);
  });

  testWidgets('the switcher lists computers with online state', (tester) async {
    final h = await Harness.create(
      servers: [
        desk(),
        desk(shortId: otherId, name: 'Laptop'),
      ],
    );
    await tester.pumpWidget(testApp(h, header()));
    await h.connect(tester);
    h.announce(fakeDiscovered(shortId: deskId));
    await tester.tap(find.byKey(const Key('status-header')));
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('switcher-$deskId')), findsOneWidget);
    expect(find.byKey(const Key('switcher-$otherId')), findsOneWidget);
    expect(find.textContaining('Online'), findsOneWidget);
    expect(find.textContaining('Offline'), findsOneWidget);
    expect(find.byIcon(Icons.check), findsOneWidget);
  });

  testWidgets('choosing another computer switches to it', (tester) async {
    final h = await Harness.create(
      servers: [
        desk(),
        desk(shortId: otherId, name: 'Laptop'),
      ],
    );
    await tester.pumpWidget(testApp(h, header()));
    await h.connect(tester);
    await tester.tap(find.byKey(const Key('status-header')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Laptop'));
    await tester.pump(const Duration(seconds: 2));
    await tester.pumpAndSettle();
    expect(h.connection.state.value.server!.shortId, otherId);
    expect(h.client.calls, contains('disconnect'));
  });

  testWidgets('swiping a computer away unpairs it after confirming', (
    tester,
  ) async {
    final h = await Harness.create(
      servers: [
        desk(),
        desk(shortId: otherId, name: 'Laptop'),
      ],
    );
    await tester.pumpWidget(testApp(h, header()));
    await h.connect(tester);
    await tester.tap(find.byKey(const Key('status-header')));
    await tester.pumpAndSettle();
    await tester.drag(
      find.byKey(const Key('switcher-$otherId')),
      const Offset(-500, 0),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(TextButton, 'Unpair'));
    await tester.pumpAndSettle();
    expect(h.services.servers.byId(otherId), isNull);
  });

  testWidgets('add computer opens the add page', (tester) async {
    final h = await Harness.create(servers: [desk()]);
    await tester.pumpWidget(testApp(h, header()));
    await tester.tap(find.byKey(const Key('status-header')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const Key('switcher-add')));
    await tester.pumpAndSettle();
    expect(find.text('route:/add'), findsOneWidget);
  });

  group('overlay', () {
    Future<void> show(WidgetTester tester, ConnState state) async {
      final h = await Harness.create(servers: [desk()]);
      await tester.pumpWidget(
        testApp(h, Scaffold(body: ConnectionOverlay(state: state))),
      );
      await tester.pump();
    }

    testWidgets('nothing when connected', (tester) async {
      await show(
        tester,
        ConnState(
          status: ConnStatus.connected,
          server: desk(),
          session: const SessionInfo(
            generation: 1,
            serverName: 'Desk',
            serverOs: OsKind.windows,
            version: 1,
            capabilities: [],
            addr: '1.2.3.4:5',
          ),
        ),
      );
      expect(find.byKey(const Key('connection-overlay')), findsNothing);
    });

    testWidgets('offline says it is retrying', (tester) async {
      await show(
        tester,
        ConnState(
          status: ConnStatus.failed,
          reason: FailReason.offline,
          server: desk(),
        ),
      );
      expect(find.text('The computer is offline. Retrying...'), findsOneWidget);
    });

    testWidgets('local network shows the guide', (tester) async {
      await show(
        tester,
        ConnState(
          status: ConnStatus.failed,
          reason: FailReason.localNetwork,
          server: desk(),
        ),
      );
      expect(find.text('Open Settings'), findsOneWidget);
      expect(find.text('Retry'), findsOneWidget);
    });

    testWidgets('removed offers pairing again', (tester) async {
      await show(
        tester,
        ConnState(
          status: ConnStatus.failed,
          reason: FailReason.removedByComputer,
          server: desk(),
        ),
      );
      await tester.tap(find.text('Pair again'));
      await tester.pumpAndSettle();
      expect(find.text('route:/add'), findsOneWidget);
    });

    testWidgets('version mismatch names the side to update', (tester) async {
      await show(
        tester,
        ConnState(
          status: ConnStatus.failed,
          reason: FailReason.updateComputer,
          server: desk(),
        ),
      );
      expect(
        find.text('Please update LanPilot on the computer.'),
        findsOneWidget,
      );
    });
  });
}
