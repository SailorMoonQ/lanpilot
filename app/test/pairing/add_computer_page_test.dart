import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/pairing/add_computer_page.dart';

import '../support/fake_client.dart';
import '../support/fake_discovery.dart';
import '../support/harness.dart';

void main() {
  testWidgets('pasting junk shows a clear error', (tester) async {
    final h = await Harness.create();
    h.client.pairResult = bridgeError(ErrorKind.invalidInput);
    await tester.pumpWidget(testApp(h, const AddComputerPage()));
    await tester.enterText(find.byKey(const Key('paste-field')), 'hello');
    await tester.tap(find.byKey(const Key('paste-pair')));
    await tester.pump();
    expect(find.text('This is not a LanPilot pairing link.'), findsOneWidget);
    expect(h.client.calls, contains('pairWithUri hello'));
  });

  testWidgets('a link inside other text pairs and opens the control page', (
    tester,
  ) async {
    final h = await Harness.create();
    await tester.pumpWidget(testApp(h, const AddComputerPage()));
    await tester.enterText(
      find.byKey(const Key('paste-field')),
      'Pair: lanpilot://pair?d=abc thanks',
    );
    await tester.tap(find.byKey(const Key('paste-pair')));
    await tester.pump(const Duration(seconds: 2));
    await tester.pumpAndSettle();
    expect(h.client.calls, contains('pairWithUri lanpilot://pair?d=abc'));
    expect(find.text('route:/control'), findsOneWidget);
    expect(h.services.servers.servers.value, hasLength(1));
  });

  testWidgets('the paste button fills the field from the clipboard', (
    tester,
  ) async {
    final h = await Harness.create();
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async => call.method == 'Clipboard.getData'
          ? <String, Object?>{'text': ' lanpilot://pair?d=zz \n'}
          : null,
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    await tester.pumpWidget(testApp(h, const AddComputerPage()));
    await tester.tap(find.byKey(const Key('paste-button')));
    await tester.pump();
    final field = tester.widget<TextField>(
      find.byKey(const Key('paste-field')),
    );
    expect(field.controller!.text, 'lanpilot://pair?d=zz');
  });

  testWidgets('nearby computers open the password page', (tester) async {
    final h = await Harness.create();
    await tester.pumpWidget(testApp(h, const AddComputerPage()));
    expect(find.text('Searching...'), findsOneWidget);
    h.announce(fakeDiscovered(shortId: 'aaaaaaaaaaaaaaaa', name: 'Studio'));
    await tester.pump();
    await tester.tap(find.byKey(const Key('nearby-aaaaaaaaaaaaaaaa')));
    await tester.pumpAndSettle();
    expect(find.text('route:/add/nearby'), findsOneWidget);
  });

  testWidgets('already paired computers are not offered as nearby', (
    tester,
  ) async {
    final h = await Harness.create(servers: [desk()]);
    await tester.pumpWidget(testApp(h, const AddComputerPage()));
    h.announce(fakeDiscovered(shortId: deskId));
    await tester.pump();
    expect(find.byKey(const Key('nearby-$deskId')), findsNothing);
  });

  testWidgets('scan opens the scan page', (tester) async {
    final h = await Harness.create();
    await tester.pumpWidget(testApp(h, const AddComputerPage()));
    await tester.tap(find.byKey(const Key('add-scan')));
    await tester.pumpAndSettle();
    expect(find.text('route:/add/scan'), findsOneWidget);
  });

  testWidgets('manual entry opens the manual page', (tester) async {
    final h = await Harness.create();
    await tester.pumpWidget(testApp(h, const AddComputerPage()));
    await tester.ensureVisible(find.byKey(const Key('add-manual')));
    await tester.tap(find.byKey(const Key('add-manual')));
    await tester.pumpAndSettle();
    expect(find.text('route:/add/manual'), findsOneWidget);
  });
}
