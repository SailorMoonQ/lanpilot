import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/pairing/scan_pairing_flow.dart';

import '../support/fake_client.dart';

void main() {
  late List<String> log;
  late DateTime clock;
  late Object? failWith;
  late bool alive;
  late ScanPairingFlow flow;

  setUp(() {
    log = [];
    clock = DateTime.utc(2026, 10, 8);
    failWith = null;
    alive = true;
    flow = ScanPairingFlow(
      pair: (raw) async {
        log.add('pair $raw');
        final f = failWith;
        if (f != null) throw f;
      },
      startScanner: () async => log.add('start'),
      stopScanner: () async => log.add('stop'),
      isAlive: () => alive,
      now: () => clock,
    );
  });

  test('a successful scan stops the scanner, pairs and stays busy', () async {
    expect(await flow.handle('a'), ScanOutcome.paired);
    expect(log, ['stop', 'pair a']);
    expect(flow.busy, isTrue);
    expect(await flow.handle('a'), ScanOutcome.ignored);
  });

  test('the same failed value is ignored for 3 s, then tried again', () async {
    failWith = bridgeError(ErrorKind.unreachable);
    expect(await flow.handle('a'), ScanOutcome.failed);
    expect(log, ['stop', 'pair a', 'start']);
    expect(flow.busy, isFalse);

    clock = clock.add(const Duration(seconds: 2));
    expect(await flow.handle('a'), ScanOutcome.ignored);
    expect(await flow.handle('b'), ScanOutcome.failed);

    log.clear();
    clock = clock.add(const Duration(seconds: 3));
    expect(await flow.handle('b'), ScanOutcome.failed);
    expect(log, ['stop', 'pair b', 'start']);
  });

  for (final kind in [ErrorKind.badToken, ErrorKind.serverKeyMismatch]) {
    test('$kind does not restart the scanner until rescan', () async {
      failWith = bridgeError(kind);
      expect(await flow.handle('a'), ScanOutcome.failed);
      expect(log, ['stop', 'pair a']);
      expect(flow.needsRescan, isTrue);
      expect(flow.error, isNotNull);
      expect(await flow.handle('b'), ScanOutcome.ignored);

      await flow.rescan();
      expect(log.last, 'start');
      expect(flow.needsRescan, isFalse);
      expect(flow.error, isNull);
    });
  }

  test('an unexpected error is kept and the scanner restarts', () async {
    failWith = StateError('boom');
    expect(await flow.handle('a'), ScanOutcome.failed);
    expect(flow.error, isA<StateError>());
    expect(flow.busy, isFalse);
    expect(log.last, 'start');
  });

  test('a scanner that fails to stop or start never wedges the flow', () async {
    final broken = ScanPairingFlow(
      pair: (_) async => throw bridgeError(ErrorKind.timeout),
      startScanner: () async => throw StateError('camera'),
      stopScanner: () async => throw StateError('camera'),
      isAlive: () => true,
      now: () => clock,
    );
    expect(await broken.handle('a'), ScanOutcome.failed);
    expect(broken.busy, isFalse);
  });

  test('a page that is gone after stopping does not pair', () async {
    alive = false;
    expect(await flow.handle('a'), ScanOutcome.ignored);
    expect(log, ['stop']);
    expect(flow.busy, isFalse);
  });

  test('a new pairing clears the old error', () async {
    failWith = bridgeError(ErrorKind.timeout);
    await flow.handle('a');
    failWith = null;
    await flow.handle('b');
    expect(flow.error, isNull);
  });
}
