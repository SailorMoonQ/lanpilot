import 'package:flutter/material.dart';

import '../bridge/lanpilot_client.dart';
import '../l10n/app_localizations.dart';

String osLabel(OsKind os, AppLocalizations l) => switch (os) {
  OsKind.windows => 'Windows',
  OsKind.linux => 'Linux',
  OsKind.macos => 'macOS',
  OsKind.ios => 'iOS',
  OsKind.android => 'Android',
  OsKind.unknown => l.osUnknown,
};

IconData osIcon(OsKind os) => switch (os) {
  OsKind.windows => Icons.desktop_windows_outlined,
  OsKind.macos => Icons.laptop_mac_outlined,
  _ => Icons.computer_outlined,
};
