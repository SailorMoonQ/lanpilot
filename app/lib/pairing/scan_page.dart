import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../app/providers.dart';
import '../l10n/app_localizations.dart';
import 'pairing_controller.dart';
import 'scan_pairing_flow.dart';

/// Scans the QR code the computer shows (parent spec 4.2). Not widget-tested:
/// the camera only exists on a device; see docs/e2e/m2-checklist.md. The
/// decisions live in [ScanPairingFlow], which is unit-tested.
class ScanPage extends ConsumerStatefulWidget {
  const ScanPage({super.key});

  @override
  ConsumerState<ScanPage> createState() => _ScanPageState();
}

class _ScanPageState extends ConsumerState<ScanPage> {
  final _scanner = MobileScannerController(
    formats: const [BarcodeFormat.qrCode],
  );
  late final ScanPairingFlow _flow;

  @override
  void initState() {
    super.initState();
    final pairing = ref.read(pairingProvider);
    _flow = ScanPairingFlow(
      pair: pairing.pairWithUri,
      startScanner: _scanner.start,
      stopScanner: _scanner.stop,
      isAlive: () => mounted,
    );
  }

  @override
  void dispose() {
    unawaited(_scanner.dispose());
    super.dispose();
  }

  Future<void> _onDetect(BarcodeCapture capture) async {
    final raw = capture.barcodes
        .map((b) => b.rawValue)
        .where(isPairingCode)
        .firstOrNull;
    if (raw == null) return;
    final pending = _flow.handle(raw);
    setState(() {});
    final outcome = await pending;
    if (!mounted) return;
    if (outcome == ScanOutcome.paired) {
      context.go('/control');
    } else {
      setState(() {});
    }
  }

  Future<void> _rescan() async {
    await _flow.rescan();
    if (mounted) setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final error = _flow.error;
    return Scaffold(
      appBar: AppBar(title: Text(l.scanQr)),
      body: Stack(
        children: [
          MobileScanner(
            controller: _scanner,
            onDetect: (c) => unawaited(_onDetect(c)),
          ),
          Align(
            alignment: Alignment.bottomCenter,
            child: SafeArea(
              child: Container(
                margin: const EdgeInsets.all(16),
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  color: Colors.black54,
                  borderRadius: BorderRadius.circular(12),
                ),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      error != null
                          ? pairingFailureText(error, l)
                          : (_flow.busy ? l.pairing : l.scanHint),
                      textAlign: TextAlign.center,
                      style: const TextStyle(color: Colors.white),
                    ),
                    if (_flow.needsRescan)
                      TextButton(
                        key: const Key('scan-retry'),
                        onPressed: () => unawaited(_rescan()),
                        child: Text(l.retry),
                      ),
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
