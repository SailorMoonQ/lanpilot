import '../bridge/lanpilot_client.dart';
import '../connection/connection_manager.dart';
import '../connection/paired_server.dart';
import '../l10n/app_localizations.dart';

const defaultPort = 45810;

final _linkPattern = RegExp(r'lanpilot://pair\?d=[A-Za-z0-9_-]+');

/// The first pairing link inside `text` (pasted text often carries more),
/// else the trimmed text so the bridge can report it as invalid.
String extractPairingLink(String text) =>
    _linkPattern.firstMatch(text)?.group(0) ?? text.trim();

String normalizeAddress(String input) {
  final t = input.trim();
  if (t.isEmpty || t.contains(':')) return t;
  return '$t:$defaultPort';
}

bool isPairingCode(String? raw) =>
    raw != null && _linkPattern.hasMatch(raw.trim());

/// Message for any failure of a pairing attempt, bridge error or not.
String pairingFailureText(Object e, AppLocalizations l) =>
    e is BridgeError ? pairingErrorText(e, l) : l.errGeneric('$e');

String pairingErrorText(BridgeError e, AppLocalizations l) => switch (e.kind) {
  ErrorKind.invalidInput => l.errInvalidCode,
  ErrorKind.timeout ||
  ErrorKind.unreachable ||
  ErrorKind.closed => l.errUnreachable,
  ErrorKind.serverKeyMismatch => l.errWrongComputer,
  ErrorKind.badToken => l.errCodeExpired,
  ErrorKind.wrongPassword => l.errWrongPassword,
  ErrorKind.passwordLocked => l.errLocked(e.retryAfterSecs),
  ErrorKind.passwordDisabled => l.errPasswordDisabled,
  ErrorKind.denied ||
  ErrorKind.notPaired ||
  ErrorKind.deviceRemoved => l.errDenied,
  ErrorKind.serverTooOld => l.statusUpdateComputer,
  ErrorKind.appTooOld => l.statusUpdateApp,
  ErrorKind.unpaired ||
  ErrorKind.notConnected ||
  ErrorKind.requestFailed ||
  ErrorKind.internal => l.errGeneric(e.message),
};

/// Pairs, then stores and connects to the new computer.
///
/// On a fresh install the first network call is a pairing, so the Local
/// Network prompt fires here, and a socket created before access was granted
/// stays blocked (M0). Like connecting (spec 4.4), a pairing that times out or
/// finds nothing reachable resets the endpoint and tries once more. The token
/// is usually still valid then: the computer typically never saw the first
/// attempt. If a timeout came from a connection that died after the token was
/// sent, the retry finds the token consumed (badToken); that is reported as
/// the original timeout, since the computer may in fact have paired.
///
/// The reset only drops the cached endpoint: a session that is live meanwhile
/// (adding a second computer, or a wrong address typed while connected) keeps
/// its socket and keeps working.
class PairingController {
  PairingController(this._client, this._connection);

  final LanPilotClient _client;
  final ConnectionManager _connection;

  Future<PairedServer> pairWithUri(String text) async {
    final link = extractPairingLink(text);
    return _connection.addPaired(
      await _resetting(() => _client.pairWithUri(link)),
    );
  }

  Future<PairedServer> pairWithPassword({
    required String address,
    required String password,
  }) async {
    final addr = normalizeAddress(address);
    return _connection.addPaired(
      await _resetting(
        () => _client.pairWithPassword(addr: addr, password: password),
      ),
    );
  }

  Future<PairedServerInfo> _resetting(
    Future<PairedServerInfo> Function() pair,
  ) async {
    try {
      return await pair();
    } on BridgeError catch (e) {
      final retryable =
          e.kind == ErrorKind.timeout || e.kind == ErrorKind.unreachable;
      if (!retryable) rethrow;
      await _client.resetEndpoint();
      try {
        return await pair();
      } on BridgeError catch (second) {
        if (e.kind == ErrorKind.timeout && second.kind == ErrorKind.badToken) {
          throw e;
        }
        rethrow;
      }
    }
  }
}
