import 'package:flutter/foundation.dart';

import '../bridge/lanpilot_client.dart';
import 'paired_server.dart';

enum ConnStatus { unpaired, connecting, connected, reconnecting, failed }

/// Why the last attempt failed (spec 4.5).
enum FailReason {
  none,
  offline,
  removedByComputer,
  updateComputer,
  updateApp,
  localNetwork,
}

@immutable
class ConnState {
  const ConnState({
    required this.status,
    this.reason = FailReason.none,
    this.server,
    this.session,
    this.showSwitcher = false,
  });

  final ConnStatus status;
  final FailReason reason;
  final PairedServer? server;
  final SessionInfo? session;

  /// Startup took too long: the UI should open the switcher once.
  final bool showSwitcher;

  ConnState copyWith({
    ConnStatus? status,
    FailReason? reason,
    PairedServer? server,
    bool? showSwitcher,
    bool clearSession = false,
  }) => ConnState(
    status: status ?? this.status,
    reason: reason ?? this.reason,
    server: server ?? this.server,
    session: clearSession ? null : session,
    showSwitcher: showSwitcher ?? this.showSwitcher,
  );
}
