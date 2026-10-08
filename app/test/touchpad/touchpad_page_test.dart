import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/settings/settings.dart';
import 'package:lanpilot/touchpad/touchpad_page.dart';

import '../support/harness.dart';

Widget page() => const Scaffold(body: TouchpadPage());

void main() {
  testWidgets('dragging moves the pointer', (tester) async {
    final h = await Harness.create(servers: [desk()]);
    await tester.pumpWidget(testApp(h, page()));
    await h.connect(tester);
    await tester.timedDrag(
      find.byKey(const Key('touchpad')),
      const Offset(120, 0),
      const Duration(milliseconds: 300),
    );
    await tester.pump(const Duration(milliseconds: 20));
    expect(h.client.calls, contains('beginGesture'));
    expect(h.client.calls.any((c) => c.startsWith('pointer')), isTrue);
  });

  testWidgets('tapping clicks after the tap delay', (tester) async {
    final h = await Harness.create(servers: [desk()]);
    await tester.pumpWidget(testApp(h, page()));
    await h.connect(tester);
    await tester.tap(find.byKey(const Key('touchpad')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(h.client.calls, isNot(contains('button left down')));
    await tester.pump(const Duration(milliseconds: 150));
    expect(
      h.client.calls,
      containsAllInOrder(['button left down', 'button left up']),
    );
  });

  testWidgets('the button strip presses and releases', (tester) async {
    final h = await Harness.create(servers: [desk()]);
    await tester.pumpWidget(testApp(h, page()));
    await h.connect(tester);
    final g = await tester.startGesture(
      tester.getCenter(find.byKey(const Key('button-right'))),
    );
    await tester.pump();
    expect(h.client.calls.last, 'button right down');
    await g.up();
    await tester.pump();
    expect(h.client.calls.last, 'button right up');
  });

  testWidgets('the overlay covers the pad until connected', (tester) async {
    final h = await Harness.create(servers: [desk()]);
    await tester.pumpWidget(testApp(h, page()));
    await tester.pump();
    expect(find.byKey(const Key('connection-overlay')), findsOneWidget);
    await h.connect(tester);
    expect(find.byKey(const Key('connection-overlay')), findsNothing);
  });

  testWidgets('the button strip can be hidden', (tester) async {
    final h = await Harness.create(
      servers: [desk()],
      settings: const Settings(showButtonBar: false),
    );
    await tester.pumpWidget(testApp(h, page()));
    await tester.pump();
    expect(find.byKey(const Key('button-left')), findsNothing);
  });

  testWidgets('landscape puts the buttons on the right', (tester) async {
    tester.view.physicalSize = const Size(2532, 1170);
    tester.view.devicePixelRatio = 3;
    addTearDown(tester.view.reset);
    final h = await Harness.create(servers: [desk()]);
    await tester.pumpWidget(testApp(h, page()));
    await tester.pump();
    final pad = tester.getCenter(find.byKey(const Key('touchpad')));
    final left = tester.getCenter(find.byKey(const Key('button-left')));
    final right = tester.getCenter(find.byKey(const Key('button-right')));
    expect(left.dx, greaterThan(pad.dx));
    expect(right.dy, greaterThan(left.dy));
  });
}
