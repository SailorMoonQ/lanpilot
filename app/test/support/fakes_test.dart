import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';

import 'fake_client.dart';

void main() {
  test('connect answers from the queue, then succeeds', () async {
    final client = FakeLanPilotClient();
    final pending = Completer<SessionInfo>();
    client.connectResults.addAll([bridgeError(ErrorKind.timeout), pending]);

    await expectLater(
      client.connect(serverKeyHex: 'k', candidates: ['a:1']),
      throwsA(
        isA<BridgeError>().having((e) => e.kind, 'kind', ErrorKind.timeout),
      ),
    );
    final second = client.connect(serverKeyHex: 'k', candidates: ['a:1']);
    pending.complete(fakeSession(9));
    expect((await second).generation, 9);
    expect(
      (await client.connect(serverKeyHex: 'k', candidates: ['b:2'])).generation,
      1,
    );
    expect(client.calls, ['connect a:1', 'connect a:1', 'connect b:2']);
  });

  test('requests fail with requestError', () async {
    final client = FakeLanPilotClient()
      ..requestError = bridgeError(ErrorKind.closed);
    await expectLater(
      client.media(MediaKind.mute),
      throwsA(isA<BridgeError>()),
    );
    expect(client.calls, ['media mute']);
  });
}
