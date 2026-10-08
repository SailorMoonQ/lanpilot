import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../app/providers.dart';
import '../l10n/app_localizations.dart';
import '../theme/app_theme.dart';
import 'conn_state.dart';
import 'switcher_sheet.dart';

/// Top bar of the control page (spec 2.2): computer name, status dot and a
/// chevron that opens the switcher.
class StatusHeader extends ConsumerWidget {
  const StatusHeader({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final connection = ref.watch(connectionProvider);
    final l = AppLocalizations.of(context);
    final p = context.palette;
    return ValueListenableBuilder<ConnState>(
      valueListenable: connection.state,
      builder: (context, s, _) => SizedBox(
        height: 52,
        child: Row(
          children: [
            const SizedBox(width: 48),
            Expanded(
              child: Center(
                child: InkWell(
                  key: const Key('status-header'),
                  borderRadius: BorderRadius.circular(18),
                  onTap: () => showSwitcher(context),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 12,
                      vertical: 6,
                    ),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Flexible(
                          child: Text(
                            s.server?.name ?? l.appTitle,
                            overflow: TextOverflow.ellipsis,
                            style: Theme.of(context).textTheme.titleMedium
                                ?.copyWith(
                                  color: p.text,
                                  fontWeight: FontWeight.w600,
                                ),
                          ),
                        ),
                        const SizedBox(width: 8),
                        StatusDot(status: s.status),
                        Icon(Icons.expand_more, color: p.secondaryText),
                      ],
                    ),
                  ),
                ),
              ),
            ),
            // Reserved for the keyboard button (spec 2.2); not shown in M2.
            const SizedBox(width: 48),
          ],
        ),
      ),
    );
  }
}

class StatusDot extends StatelessWidget {
  const StatusDot({super.key, required this.status});

  final ConnStatus status;

  @override
  Widget build(BuildContext context) {
    final color = switch (status) {
      ConnStatus.connected => const Color(0xFF34C759),
      ConnStatus.connecting ||
      ConnStatus.reconnecting => const Color(0xFFFF9F0A),
      ConnStatus.failed => const Color(0xFFFF453A),
      ConnStatus.unpaired => const Color(0xFF8E8E93),
    };
    return Container(
      key: Key('status-${status.name}'),
      width: 8,
      height: 8,
      decoration: BoxDecoration(color: color, shape: BoxShape.circle),
    );
  }
}
