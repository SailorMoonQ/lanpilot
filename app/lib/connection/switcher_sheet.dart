import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../app/labels.dart';
import '../app/providers.dart';
import '../l10n/app_localizations.dart';
import '../theme/app_theme.dart';
import 'paired_server.dart';

Future<void> showSwitcher(BuildContext context) => showModalBottomSheet<void>(
  context: context,
  showDragHandle: true,
  isScrollControlled: true,
  builder: (_) => const SwitcherSheet(),
);

/// Paired computers (spec 2.4): online state, current one checked, swipe left
/// to unpair, then "Add computer" and "Settings".
class SwitcherSheet extends ConsumerWidget {
  const SwitcherSheet({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final store = ref.watch(serverStoreProvider);
    final discovery = ref.watch(discoveryProvider);
    final connection = ref.watch(connectionProvider);
    final l = AppLocalizations.of(context);
    final p = context.palette;
    return SafeArea(
      child: ListenableBuilder(
        listenable: Listenable.merge([
          store.servers,
          discovery.devices,
          connection.state,
        ]),
        builder: (context, _) {
          final current = connection.state.value.server?.shortId;
          return Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(20, 0, 20, 8),
                child: Text(
                  l.switcherTitle,
                  style: Theme.of(context).textTheme.titleLarge,
                ),
              ),
              for (final s in store.servers.value)
                Dismissible(
                  key: Key('switcher-${s.shortId}'),
                  direction: DismissDirection.endToStart,
                  background: Container(
                    color: const Color(0xFFFF3B30),
                    alignment: Alignment.centerRight,
                    padding: const EdgeInsets.symmetric(horizontal: 20),
                    child: Text(
                      l.unpair,
                      style: const TextStyle(color: Colors.white),
                    ),
                  ),
                  confirmDismiss: (_) => _confirmUnpair(context, s),
                  onDismissed: (_) => unawaited(connection.unpair(s)),
                  child: ListTile(
                    leading: Icon(osIcon(s.os)),
                    title: Text(s.name),
                    subtitle: Text(
                      '${discovery.lookup(s.shortId) != null ? l.online : l.offline}'
                      ' · ${osLabel(s.os, l)}',
                    ),
                    trailing: s.shortId == current
                        ? Icon(Icons.check, color: p.accent)
                        : null,
                    onTap: () {
                      Navigator.of(context).pop();
                      if (s.shortId != current) {
                        unawaited(connection.switchTo(s));
                      }
                    },
                  ),
                ),
              const Divider(),
              ListTile(
                key: const Key('switcher-add'),
                leading: const Icon(Icons.add),
                title: Text(l.addComputer),
                onTap: () => _go(context, '/add'),
              ),
              ListTile(
                key: const Key('switcher-settings'),
                leading: const Icon(Icons.settings_outlined),
                title: Text(l.settings),
                onTap: () => _go(context, '/settings'),
              ),
            ],
          );
        },
      ),
    );
  }

  void _go(BuildContext context, String location) {
    final router = GoRouter.of(context);
    Navigator.of(context).pop();
    unawaited(router.push(location));
  }

  Future<bool> _confirmUnpair(BuildContext context, PairedServer server) async {
    final l = AppLocalizations.of(context);
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l.unpairConfirm(server.name)),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: Text(l.cancel),
          ),
          TextButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(l.unpair),
          ),
        ],
      ),
    );
    return ok ?? false;
  }
}
