import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../app/providers.dart';
import '../bridge/lanpilot_client.dart';
import '../connection/conn_state.dart';
import '../l10n/app_localizations.dart';
import '../theme/app_theme.dart';

String mediaLabel(MediaKind kind, AppLocalizations l) => switch (kind) {
  MediaKind.playPause => l.mediaPlayPause,
  MediaKind.next => l.mediaNext,
  MediaKind.previous => l.mediaPrevious,
  MediaKind.volumeUp => l.mediaVolumeUp,
  MediaKind.volumeDown => l.mediaVolumeDown,
  MediaKind.mute => l.mediaMute,
};

class MediaPage extends ConsumerWidget {
  const MediaPage({super.key});

  static const _buttons = [
    (MediaKind.previous, Icons.skip_previous_rounded),
    (MediaKind.playPause, Icons.play_arrow_rounded),
    (MediaKind.next, Icons.skip_next_rounded),
    (MediaKind.volumeDown, Icons.volume_down_rounded),
    (MediaKind.mute, Icons.volume_off_rounded),
    (MediaKind.volumeUp, Icons.volume_up_rounded),
  ];

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final connection = ref.watch(connectionProvider);
    final l = AppLocalizations.of(context);
    final p = context.palette;
    return ValueListenableBuilder<ConnState>(
      valueListenable: connection.state,
      builder: (context, state, _) {
        final enabled = state.status == ConnStatus.connected;
        return GridView.count(
          crossAxisCount: 3,
          padding: const EdgeInsets.all(24),
          mainAxisSpacing: 16,
          crossAxisSpacing: 16,
          children: [
            for (final (kind, icon) in _buttons)
              Material(
                color: p.controlFill,
                borderRadius: BorderRadius.circular(20),
                child: InkWell(
                  key: Key('media-${kind.name}'),
                  borderRadius: BorderRadius.circular(20),
                  onTap: enabled
                      ? () async {
                          unawaited(HapticFeedback.selectionClick());
                          try {
                            await connection.input.media(kind);
                          } on BridgeError {
                            if (!context.mounted) return;
                            ScaffoldMessenger.of(context).showSnackBar(
                              SnackBar(content: Text(l.actionFailed)),
                            );
                          }
                        }
                      : null,
                  child: Opacity(
                    opacity: enabled ? 1 : 0.4,
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Icon(icon, size: 36, color: p.text),
                        const SizedBox(height: 6),
                        Text(
                          mediaLabel(kind, l),
                          textAlign: TextAlign.center,
                          style: TextStyle(
                            color: p.secondaryText,
                            fontSize: 12,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
          ],
        );
      },
    );
  }
}
