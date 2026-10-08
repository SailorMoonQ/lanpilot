import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../theme/app_theme.dart';

/// Placeholder until M3 (spec 1.1).
class ShortcutsPage extends StatelessWidget {
  const ShortcutsPage({super.key});

  @override
  Widget build(BuildContext context) {
    final p = context.palette;
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.bolt_rounded, size: 48, color: p.secondaryText),
          const SizedBox(height: 12),
          Text(
            AppLocalizations.of(context).comingSoon,
            style: TextStyle(color: p.secondaryText, fontSize: 16),
          ),
        ],
      ),
    );
  }
}
