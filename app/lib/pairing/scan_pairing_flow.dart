import 'dart:async';

import '../bridge/lanpilot_client.dart';

enum ScanOutcome { ignored, paired, failed }

/// The decision logic of the scan page, apart from the camera so it can be
/// tested: one pairing at a time, no scanner restart after a QR code the
/// computer rejected for good, and a failed value is ignored for a while so a
/// code still in view does not loop.
class ScanPairingFlow {
  ScanPairingFlow({
    required this.pair,
    required this.startScanner,
    required this.stopScanner,
    required this.isAlive,
    this.now = DateTime.now,
    this.ignoreFailedFor = const Duration(seconds: 3),
  });

  final Future<void> Function(String raw) pair;
  final Future<void> Function() startScanner;
  final Future<void> Function() stopScanner;
  final bool Function() isAlive;
  final DateTime Function() now;
  final Duration ignoreFailedFor;

  bool busy = false;

  /// The last failure, until the next pairing starts or [rescan] is called.
  Object? error;

  /// True when the scanner stays stopped until the user asks ([rescan]).
  bool needsRescan = false;

  String? _failedRaw;
  DateTime? _failedAt;

  Future<ScanOutcome> handle(String raw) async {
    if (busy || needsRescan) return ScanOutcome.ignored;
    final failedAt = _failedAt;
    if (raw == _failedRaw &&
        failedAt != null &&
        now().difference(failedAt) < ignoreFailedFor) {
      return ScanOutcome.ignored;
    }
    busy = true;
    error = null;
    try {
      await _safely(stopScanner);
      if (!isAlive()) {
        busy = false;
        return ScanOutcome.ignored;
      }
      await pair(raw);
      return ScanOutcome.paired; // stays busy: the page is navigating away
    } on Object catch (e) {
      error = e;
      _failedRaw = raw;
      _failedAt = now();
      final rejected =
          e is BridgeError &&
          (e.kind == ErrorKind.badToken ||
              e.kind == ErrorKind.serverKeyMismatch);
      if (rejected) {
        needsRescan = true;
      } else if (isAlive()) {
        await _safely(startScanner);
      }
      busy = false;
      return ScanOutcome.failed;
    }
  }

  /// Clears the error and starts the scanner again.
  Future<void> rescan() async {
    error = null;
    needsRescan = false;
    await _safely(startScanner);
  }

  Future<void> _safely(Future<void> Function() action) async {
    try {
      await action();
    } on Object {
      // A camera that will not stop or start must not wedge the page.
    }
  }
}
