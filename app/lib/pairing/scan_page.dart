import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../app/providers.dart';
import '../bridge/lanpilot_client.dart';
import '../l10n/app_localizations.dart';
import 'pairing_controller.dart';

/// Scans the QR code the computer shows (parent spec 4.2). Not widget-tested:
/// the camera only exists on a device; see docs/e2e/m2-checklist.md.
class ScanPage extends ConsumerStatefulWidget {
  const ScanPage({super.key});

  @override
  ConsumerState<ScanPage> createState() => _ScanPageState();
}

class _ScanPageState extends ConsumerState<ScanPage> {
  final _scanner = MobileScannerController(
    formats: const [BarcodeFormat.qrCode],
  );
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    unawaited(_scanner.dispose());
    super.dispose();
  }

  Future<void> _onDetect(BarcodeCapture capture) async {
    if (_busy) return;
    final raw = capture.barcodes
        .map((b) => b.rawValue)
        .where(isPairingCode)
        .firstOrNull;
    if (raw == null) return;
    final l = AppLocalizations.of(context);
    setState(() => _busy = true);
    await _scanner.stop();
    try {
      await ref.read(pairingProvider).pairWithUri(raw);
      if (mounted) context.go('/control');
    } on BridgeError catch (e) {
      if (!mounted) return;
      setState(() {
        _error = pairingErrorText(e, l);
        _busy = false;
      });
      await _scanner.start();
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
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
                child: Text(
                  _error ?? (_busy ? l.pairing : l.scanHint),
                  textAlign: TextAlign.center,
                  style: const TextStyle(color: Colors.white),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
