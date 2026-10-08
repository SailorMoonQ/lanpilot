import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:url_launcher/url_launcher.dart';

import '../app/providers.dart';
import '../l10n/app_localizations.dart';
import 'conn_state.dart';

String overlayMessage(ConnState s, AppLocalizations l) => switch (s.status) {
  ConnStatus.connecting => l.statusConnecting,
  ConnStatus.reconnecting => l.statusReconnecting,
  ConnStatus.unpaired => l.addComputerHint,
  ConnStatus.connected => '',
  ConnStatus.failed => switch (s.reason) {
    FailReason.none || FailReason.offline => l.statusOffline,
    FailReason.removedByComputer => l.statusRemoved,
    FailReason.updateComputer => l.statusUpdateComputer,
    FailReason.updateApp => l.statusUpdateApp,
    FailReason.localNetwork => l.localNetworkBody,
  },
};

/// Covers the touchpad whenever it is not connected (spec 2.2, 4.5); touches
/// land on the overlay, so no gesture is sent.
class ConnectionOverlay extends ConsumerWidget {
  const ConnectionOverlay({super.key, required this.state});

  final ConnState state;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (state.status == ConnStatus.connected) return const SizedBox.shrink();
    final l = AppLocalizations.of(context);
    final connection = ref.read(connectionProvider);
    final busy =
        state.status == ConnStatus.connecting ||
        state.status == ConnStatus.reconnecting ||
        (state.status == ConnStatus.failed &&
            state.reason == FailReason.offline);
    const white = TextStyle(color: Colors.white);
    final Widget content;
    if (state.status == ConnStatus.failed &&
        state.reason == FailReason.localNetwork) {
      content = LocalNetworkGuide(
        onRetry: () => unawaited(connection.retryNow()),
      );
    } else {
      content = Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (busy) const CircularProgressIndicator.adaptive(),
          if (busy) const SizedBox(height: 12),
          Text(
            overlayMessage(state, l),
            textAlign: TextAlign.center,
            style: white,
          ),
          if (state.reason == FailReason.removedByComputer ||
              state.status == ConnStatus.unpaired)
            TextButton(
              onPressed: () => unawaited(context.push('/add')),
              child: Text(l.pairAgain),
            ),
        ],
      );
    }
    return Container(
      key: const Key('connection-overlay'),
      alignment: Alignment.center,
      padding: const EdgeInsets.all(24),
      decoration: BoxDecoration(
        color: Colors.black.withValues(alpha: 0.55),
        borderRadius: BorderRadius.circular(20),
      ),
      child: content,
    );
  }
}

/// Spec 4.4: never connected and still timing out.
class LocalNetworkGuide extends StatelessWidget {
  const LocalNetworkGuide({super.key, required this.onRetry});

  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    const white = TextStyle(color: Colors.white);
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        const Icon(Icons.wifi_lock_outlined, color: Colors.white, size: 40),
        const SizedBox(height: 12),
        Text(
          l.localNetworkTitle,
          style: white.copyWith(fontSize: 18, fontWeight: FontWeight.w600),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 8),
        Text(l.localNetworkBody, style: white, textAlign: TextAlign.center),
        const SizedBox(height: 16),
        Wrap(
          spacing: 12,
          children: [
            FilledButton(
              onPressed: () => unawaited(launchUrl(Uri.parse('app-settings:'))),
              child: Text(l.openSettings),
            ),
            OutlinedButton(onPressed: onRetry, child: Text(l.retry)),
          ],
        ),
      ],
    );
  }
}
