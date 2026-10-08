import '../bridge/lanpilot_client.dart';

/// Sends input while connected and remembers held buttons, so they can be
/// released before the app goes to the background (spec 4.3). Requests are
/// sent one after another in call order: an unawaited button up must never
/// overtake its button down.
class InputController {
  InputController(this._client, this._isConnected);

  final LanPilotClient _client;
  final bool Function() _isConnected;
  final _held = <MouseButtonKind>{};
  Future<void> _tail = Future.value();

  void beginGesture() => _datagram(_client.beginGesture);

  void pointer(double dx, double dy) =>
      _datagram(() => _client.sendPointer(dx, dy));

  void scroll(double dx, double dy) =>
      _datagram(() => _client.sendScroll(dx, dy));

  Future<void> button(MouseButtonKind button, {required bool down}) async {
    if (down) {
      _held.add(button);
    } else {
      _held.remove(button);
    }
    if (!_isConnected()) return;
    try {
      await _inOrder(() => _client.button(button, down: down));
    } on BridgeError {
      // The agent releases everything when a session ends anyway.
    }
  }

  Future<void> click(MouseButtonKind button) async {
    await this.button(button, down: true);
    await this.button(button, down: false);
  }

  /// Throws [BridgeError] so the media page can tell the user.
  Future<void> media(MediaKind action) async {
    if (!_isConnected()) return;
    await _inOrder(() => _client.media(action));
  }

  Future<void> zoom(double steps) async {
    if (!_isConnected() || steps == 0) return;
    try {
      await _inOrder(() => _client.zoom(steps));
    } on BridgeError {
      // Best effort, like pointer motion.
    }
  }

  Future<void> releaseAll() async {
    final held = _held.toList();
    _held.clear();
    if (!_isConnected()) return;
    for (final b in held) {
      try {
        await _inOrder(() => _client.button(b, down: false));
      } on BridgeError {
        // Disconnecting releases it on the computer too.
      }
    }
  }

  /// Runs `op` after every request issued before it.
  Future<void> _inOrder(Future<void> Function() op) {
    final next = _tail.then((_) => op());
    _tail = next.then<void>((_) {}, onError: (Object _) {});
    return next;
  }

  void _datagram(void Function() send) {
    if (!_isConnected()) return;
    try {
      send();
    } on BridgeError {
      // Datagrams are best effort; the next one carries the totals.
    }
  }
}
