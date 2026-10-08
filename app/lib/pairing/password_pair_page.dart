import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../app/providers.dart';
import '../bridge/lanpilot_client.dart';
import '../l10n/app_localizations.dart';
import 'pairing_controller.dart';

/// Password pairing (parent spec 4.3) with a nearby computer, or with an
/// address typed by hand when `device` is null.
class PasswordPairPage extends ConsumerStatefulWidget {
  const PasswordPairPage({super.key, this.device});

  final DiscoveredInfo? device;

  @override
  ConsumerState<PasswordPairPage> createState() => _PasswordPairPageState();
}

class _PasswordPairPageState extends ConsumerState<PasswordPairPage> {
  final _address = TextEditingController();
  final _password = TextEditingController();
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _address.dispose();
    _password.dispose();
    super.dispose();
  }

  Future<void> _pair() async {
    final l = AppLocalizations.of(context);
    final device = widget.device;
    final address = device != null && device.addrs.isNotEmpty
        ? device.addrs.first
        : _address.text;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await ref
          .read(pairingProvider)
          .pairWithPassword(address: address, password: _password.text);
      if (mounted) context.go('/control');
    } on BridgeError catch (e) {
      if (mounted) setState(() => _error = pairingErrorText(e, l));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final device = widget.device;
    return Scaffold(
      appBar: AppBar(title: Text(device?.name ?? l.manualIp)),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            if (device == null) ...[
              TextField(
                key: const Key('address-field'),
                controller: _address,
                keyboardType: TextInputType.url,
                autocorrect: false,
                decoration: InputDecoration(
                  labelText: l.address,
                  hintText: '192.168.1.10',
                  border: const OutlineInputBorder(),
                ),
              ),
              const SizedBox(height: 12),
            ],
            TextField(
              key: const Key('password-field'),
              controller: _password,
              obscureText: true,
              autocorrect: false,
              decoration: InputDecoration(
                labelText: l.password,
                errorText: _error,
                errorMaxLines: 3,
                border: const OutlineInputBorder(),
              ),
              onSubmitted: (_) => unawaited(_pair()),
            ),
            const SizedBox(height: 16),
            FilledButton(
              key: const Key('password-pair'),
              onPressed: _busy ? null : () => unawaited(_pair()),
              child: _busy
                  ? const SizedBox.square(
                      dimension: 18,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : Text(l.pairWithPassword),
            ),
          ],
        ),
      ),
    );
  }
}
