import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/l10n/app_localizations.dart';
import 'package:lanpilot/pairing/pairing_controller.dart';

import '../support/fake_client.dart';
import '../support/harness.dart';

void main() {
  test('extractPairingLink finds the link inside other text', () {
    expect(
      extractPairingLink('  open lanpilot://pair?d=Ab_-9 please\n'),
      'lanpilot://pair?d=Ab_-9',
    );
    expect(
      extractPairingLink('\n lanpilot://pair?d=xyz\n'),
      'lanpilot://pair?d=xyz',
    );
    expect(extractPairingLink(' hello '), 'hello');
  });

  test('normalizeAddress adds the default port', () {
    expect(normalizeAddress(' 10.0.0.5 '), '10.0.0.5:45810');
    expect(normalizeAddress('10.0.0.5:5000'), '10.0.0.5:5000');
    expect(normalizeAddress(''), '');
  });

  test('isPairingCode', () {
    expect(isPairingCode('lanpilot://pair?d=x'), isTrue);
    expect(isPairingCode(' lanpilot://pair?d=x '), isTrue);
    expect(isPairingCode('https://example.com'), isFalse);
    expect(isPairingCode(null), isFalse);
    expect(isPairingCode('lanpilot://pair?x'), isFalse);
    expect(isPairingCode('lanpilot://pair?d='), isFalse);
  });

  group('PairingController', () {
    late Harness h;
    setUp(() async => h = await Harness.create());

    test('a timed-out pairing resets the endpoint and tries again', () async {
      h.client.pairResults.add(bridgeError(ErrorKind.timeout));
      final server = await PairingController(
        h.client,
        h.connection,
      ).pairWithUri('lanpilot://pair?d=abc');
      expect(server.shortId, fakeServerInfo().shortId);
      expect(
        h.client.calls.where(
          (c) => c.startsWith('pair') || c == 'resetEndpoint',
        ),
        [
          'pairWithUri lanpilot://pair?d=abc',
          'resetEndpoint',
          'pairWithUri lanpilot://pair?d=abc',
        ],
      );
    });

    test('password pairing retries the same way, once', () async {
      h.client.pairResults.addAll([
        bridgeError(ErrorKind.unreachable),
        bridgeError(ErrorKind.unreachable),
      ]);
      await expectLater(
        PairingController(
          h.client,
          h.connection,
        ).pairWithPassword(address: '10.0.0.5', password: 'secret1'),
        throwsA(
          isA<BridgeError>().having(
            (e) => e.kind,
            'kind',
            ErrorKind.unreachable,
          ),
        ),
      );
      expect(h.client.calls.where((c) => c == 'resetEndpoint'), hasLength(1));
      expect(
        h.client.calls.where((c) => c.startsWith('pairWithPassword')),
        hasLength(2),
      );
    });

    test('a consumed token after a timeout reports the timeout', () async {
      h.client.pairResults.addAll([
        bridgeError(ErrorKind.timeout),
        bridgeError(ErrorKind.badToken),
      ]);
      await expectLater(
        PairingController(
          h.client,
          h.connection,
        ).pairWithUri('lanpilot://pair?d=abc'),
        throwsA(
          isA<BridgeError>().having((e) => e.kind, 'kind', ErrorKind.timeout),
        ),
      );
      expect(
        h.client.calls.where(
          (c) => c.startsWith('pair') || c == 'resetEndpoint',
        ),
        [
          'pairWithUri lanpilot://pair?d=abc',
          'resetEndpoint',
          'pairWithUri lanpilot://pair?d=abc',
        ],
      );
    });

    test('other errors are not retried', () async {
      h.client.pairResults.add(bridgeError(ErrorKind.badToken));
      await expectLater(
        PairingController(
          h.client,
          h.connection,
        ).pairWithUri('lanpilot://pair?d=abc'),
        throwsA(isA<BridgeError>()),
      );
      expect(h.client.calls, isNot(contains('resetEndpoint')));
    });
  });

  test('every error has a message, in both languages', () {
    for (final locale in const [Locale('en'), Locale('zh')]) {
      final l = lookupAppLocalizations(locale);
      for (final kind in ErrorKind.values) {
        expect(
          pairingErrorText(bridgeError(kind), l),
          isNotEmpty,
          reason: '$kind',
        );
      }
      const locked = BridgeError(
        kind: ErrorKind.passwordLocked,
        message: '',
        retryAfterSecs: 90,
      );
      expect(pairingErrorText(locked, l), contains('90'));
    }
  });
}
