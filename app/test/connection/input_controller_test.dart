import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/connection/input_controller.dart';

import '../support/fake_client.dart';

void main() {
  test('input is dropped while not connected', () async {
    final client = FakeLanPilotClient();
    final input = InputController(client, () => false);
    input
      ..beginGesture()
      ..pointer(1, 2)
      ..scroll(0, 1);
    await input.click(MouseButtonKind.left);
    expect(client.calls, isEmpty);
  });

  test('releaseAll lifts every held button once', () async {
    final client = FakeLanPilotClient();
    final input = InputController(client, () => true);
    await input.button(MouseButtonKind.left, down: true);
    await input.button(MouseButtonKind.right, down: true);
    await input.button(MouseButtonKind.right, down: false);
    client.calls.clear();
    await input.releaseAll();
    await input.releaseAll();
    expect(client.calls, ['button left up']);
  });

  test('unawaited requests keep their order', () async {
    final client = FakeLanPilotClient();
    final input = InputController(client, () => true);
    final down = input.button(MouseButtonKind.left, down: true);
    final up = input.button(MouseButtonKind.left, down: false);
    await Future.wait([down, up]);
    expect(client.calls, ['button left down', 'button left up']);
  });

  test('datagram errors are swallowed, media errors are not', () async {
    final client = FakeLanPilotClient()
      ..requestError = bridgeError(ErrorKind.closed);
    final input = InputController(client, () => true);
    await input.click(MouseButtonKind.left);
    await expectLater(input.media(MediaKind.mute), throwsA(isA<BridgeError>()));
  });
}
