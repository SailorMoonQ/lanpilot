import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../app/labels.dart';
import '../app/providers.dart';
import '../bridge/lanpilot_client.dart';
import '../l10n/app_localizations.dart';
import '../theme/app_theme.dart';
import 'pairing_controller.dart';

/// Add a computer (spec 1.1): scan, paste a link, a nearby computer plus
/// password, or a manual IP plus password.
class AddComputerPage extends ConsumerStatefulWidget {
  const AddComputerPage({super.key});

  @override
  ConsumerState<AddComputerPage> createState() => _AddComputerPageState();
}

class _AddComputerPageState extends ConsumerState<AddComputerPage> {
  final _link = TextEditingController();
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _link.dispose();
    super.dispose();
  }

  Future<void> _paste() async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    setState(() => _link.text = data?.text?.trim() ?? '');
  }

  Future<void> _pair() async {
    final l = AppLocalizations.of(context);
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await ref.read(pairingProvider).pairWithUri(_link.text);
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
    final p = context.palette;
    final heading = Theme.of(context).textTheme.titleSmall
        ?.copyWith(color: p.secondaryText);
    return Scaffold(
      appBar: AppBar(
        title: Text(l.addComputer),
        actions: [
          IconButton(
            key: const Key('add-settings'),
            icon: const Icon(Icons.settings_outlined),
            onPressed: () => unawaited(context.push('/settings')),
          ),
        ],
      ),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            Text(l.addComputerHint, style: TextStyle(color: p.secondaryText)),
            const SizedBox(height: 16),
            Card(
              child: ListTile(
                key: const Key('add-scan'),
                leading: const Icon(Icons.qr_code_scanner),
                title: Text(l.scanQr),
                trailing: const Icon(Icons.chevron_right),
                onTap: () => unawaited(context.push('/add/scan')),
              ),
            ),
            const SizedBox(height: 24),
            Text(l.pasteLink, style: heading),
            const SizedBox(height: 8),
            TextField(
              key: const Key('paste-field'),
              controller: _link,
              minLines: 1,
              maxLines: 3,
              decoration: InputDecoration(
                hintText: 'lanpilot://pair?d=...',
                errorText: _error,
                errorMaxLines: 3,
                border: const OutlineInputBorder(),
                suffixIcon: IconButton(
                  key: const Key('paste-button'),
                  tooltip: l.paste,
                  icon: const Icon(Icons.content_paste),
                  onPressed: () => unawaited(_paste()),
                ),
              ),
            ),
            const SizedBox(height: 8),
            FilledButton(
              key: const Key('paste-pair'),
              onPressed: _busy ? null : () => unawaited(_pair()),
              child: _busy
                  ? const SizedBox.square(
                      dimension: 18,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : Text(l.pair),
            ),
            const SizedBox(height: 24),
            Text(l.nearbyComputers, style: heading),
            const SizedBox(height: 8),
            const _NearbyList(),
            const SizedBox(height: 16),
            Card(
              child: ListTile(
                key: const Key('add-manual'),
                leading: const Icon(Icons.edit_outlined),
                title: Text(l.manualIp),
                trailing: const Icon(Icons.chevron_right),
                onTap: () => unawaited(context.push('/add/manual')),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Computers announcing themselves that are not paired yet.
class _NearbyList extends ConsumerWidget {
  const _NearbyList();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final discovery = ref.watch(discoveryProvider);
    final store = ref.watch(serverStoreProvider);
    final l = AppLocalizations.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([discovery.devices, store.servers]),
      builder: (context, _) {
        final paired = {for (final s in store.servers.value) s.shortId};
        final nearby = [
          for (final d in discovery.devices.value.values)
            if (!paired.contains(d.shortId)) d,
        ];
        if (nearby.isEmpty) {
          return ListTile(
            leading: const SizedBox.square(
              dimension: 18,
              child: CircularProgressIndicator(strokeWidth: 2),
            ),
            title: Text(l.searching),
          );
        }
        return Column(
          children: [
            for (final d in nearby)
              Card(
                child: ListTile(
                  key: Key('nearby-${d.shortId}'),
                  leading: Icon(osIcon(d.os)),
                  title: Text(d.name),
                  subtitle: Text(osLabel(d.os, l)),
                  trailing: const Icon(Icons.lock_outline),
                  onTap: () => unawaited(context.push('/add/nearby', extra: d)),
                ),
              ),
          ],
        );
      },
    );
  }
}
