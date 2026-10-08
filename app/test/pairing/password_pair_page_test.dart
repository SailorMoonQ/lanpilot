import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/pairing/password_pair_page.dart';

import '../support/fake_client.dart';
import '../support/fake_discovery.dart';
import '../support/harness.dart';

void main() {
  testWidgets('manual entry pairs with the default port', (tester) async {
    final h = await Harness.create();
    await tester.pumpWidget(testApp(h, const PasswordPairPage()));
    await tester.enterText(find.byKey(const Key('address-field')), '10.0.0.5');
    await tester.enterText(find.byKey(const Key('password-field')), 'secret1');
    await tester.tap(find.byKey(const Key('password-pair')));
    await tester.pump(const Duration(seconds: 2));
    await tester.pumpAndSettle();
    expect(h.client.calls, contains('pairWithPassword 10.0.0.5:45810 secret1'));
    expect(find.text('route:/control'), findsOneWidget);
  });

  testWidgets('a nearby computer uses its announced address', (tester) async {
    final h = await Harness.create();
    await tester.pumpWidget(
      testApp(
        h,
        PasswordPairPage(
          device: fakeDiscovered(addrs: const ['10.0.0.9:45810']),
        ),
      ),
    );
    expect(find.byKey(const Key('address-field')), findsNothing);
    await tester.enterText(find.byKey(const Key('password-field')), 'secret1');
    await tester.tap(find.byKey(const Key('password-pair')));
    await tester.pump(const Duration(seconds: 2));
    expect(h.client.calls, contains('pairWithPassword 10.0.0.9:45810 secret1'));
  });

  testWidgets('a wrong password is explained', (tester) async {
    final h = await Harness.create();
    h.client.pairResult = bridgeError(ErrorKind.wrongPassword);
    await tester.pumpWidget(testApp(h, const PasswordPairPage()));
    await tester.enterText(find.byKey(const Key('address-field')), '10.0.0.5');
    await tester.enterText(find.byKey(const Key('password-field')), 'nope');
    await tester.tap(find.byKey(const Key('password-pair')));
    await tester.pump();
    expect(find.text('Wrong password.'), findsOneWidget);
  });

  testWidgets('submitting twice pairs once', (tester) async {
    final h = await Harness.create();
    h.client.pairGate = Completer<void>();
    await tester.pumpWidget(testApp(h, const PasswordPairPage()));
    await tester.enterText(find.byKey(const Key('address-field')), '10.0.0.5');
    await tester.enterText(find.byKey(const Key('password-field')), 'secret1');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();
    expect(
      h.client.calls.where((c) => c.startsWith('pairWithPassword')),
      hasLength(1),
    );
    h.client.pairGate!.complete();
    await tester.pump(const Duration(seconds: 2));
    await tester.pumpAndSettle();
  });

  testWidgets('an unexpected error is shown, not swallowed', (tester) async {
    final h = await Harness.create();
    h.client.pairResult = StateError('boom');
    await tester.pumpWidget(testApp(h, const PasswordPairPage()));
    await tester.enterText(find.byKey(const Key('address-field')), '10.0.0.5');
    await tester.enterText(find.byKey(const Key('password-field')), 'x');
    await tester.tap(find.byKey(const Key('password-pair')));
    await tester.pump();
    expect(find.textContaining('boom'), findsOneWidget);
  });
}
