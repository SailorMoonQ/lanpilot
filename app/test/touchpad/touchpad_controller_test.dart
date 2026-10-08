import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/connection/input_controller.dart';
import 'package:lanpilot/settings/settings.dart';
import 'package:lanpilot/touchpad/gesture_machine.dart';
import 'package:lanpilot/touchpad/touchpad_controller.dart';
import 'package:lanpilot/touchpad/tuning.dart';

import '../support/fake_client.dart';

Duration ms(int v) => Duration(milliseconds: v);

void main() {
  late FakeLanPilotClient client;
  late Settings settings;
  late List<HapticKind> haptics;
  late TouchpadController c;

  setUp(() {
    client = FakeLanPilotClient();
    settings = const Settings(accel: AccelPreset.off);
    haptics = [];
    c = TouchpadController(
      input: InputController(client, () => true),
      settings: () => settings,
      onHaptic: haptics.add,
    );
  });

  test('motion in one frame is merged into one datagram', () {
    c.beginGesture();
    c.move(const Offset(5, 0), ms(0));
    c.move(const Offset(5, 0), ms(8));
    expect(client.calls.where((x) => x.startsWith('pointer')), isEmpty);
    c.onFrame(ms(16));
    final px = 10 * Tuning.basePxPerPt;
    expect(client.calls, [
      'beginGesture',
      'pointer ${px.toStringAsFixed(1)},0.0',
    ]);
    expect(c.needsFrames, isFalse);
  });

  test('scroll follows the natural scrolling setting', () {
    c.scroll(const Offset(0, Tuning.scrollPtPerNotch));
    c.onFrame(ms(16));
    expect(client.calls.last, 'scroll 0.00,1.00');
    settings = settings.copyWith(naturalScroll: false);
    c.scroll(const Offset(0, Tuning.scrollPtPerNotch));
    c.onFrame(ms(32));
    expect(client.calls.last, 'scroll 0.00,-1.00');
  });

  test('a click flushes pending motion first', () async {
    c.move(const Offset(5, 0), ms(0));
    c.click(MouseButtonKind.left);
    await Future<void>.delayed(Duration.zero);
    expect(client.calls.first, startsWith('pointer'));
    expect(client.calls.skip(1), ['button left down', 'button left up']);
  });

  test('a fling keeps scrolling until it stops, and a touch stops it', () {
    c.scrollEnd(const Offset(0, 1000));
    var frame = 0;
    c.onFrame(ms(0));
    for (var i = 1; i <= 5; i++) {
      c.onFrame(ms(i * 16));
      frame = i;
    }
    final scrolls = client.calls.where((x) => x.startsWith('scroll')).length;
    expect(scrolls, frame);
    expect(c.needsFrames, isTrue);
    c.stopInertia();
    c.onFrame(ms(200));
    expect(client.calls.where((x) => x.startsWith('scroll')).length, scrolls);
  });

  test('pinch sends zoom steps', () async {
    c.zoom(2);
    c.onFrame(ms(16));
    await Future<void>.delayed(Duration.zero);
    expect(
      client.calls.last,
      'zoom ${Tuning.zoomStepsPerDoubling.toStringAsFixed(2)}',
    );
  });

  test('haptics follow the setting', () {
    c.haptic(HapticKind.tap);
    settings = settings.copyWith(haptics: false);
    c.haptic(HapticKind.tap);
    expect(haptics, [HapticKind.tap]);
  });

  test('zoom has at most one request in flight', () async {
    final gate = Completer<void>();
    client.zoomGate = gate;
    c.zoom(2);
    c.onFrame(ms(16));
    await Future<void>.delayed(Duration.zero);
    expect(client.calls.last, 'zoom 4.00');
    for (var i = 2; i <= 4; i++) {
      c.zoom(2);
      c.onFrame(ms(i * 16));
    }
    expect(client.calls.where((x) => x.startsWith('zoom')), ['zoom 4.00']);
    expect(c.needsFrames, isTrue);
    gate.complete();
    await Future<void>.delayed(Duration.zero);
    c.onFrame(ms(100));
    await Future<void>.delayed(Duration.zero);
    expect(client.calls.where((x) => x.startsWith('zoom')), [
      'zoom 4.00',
      'zoom 12.00',
    ]);
    expect(c.needsFrames, isFalse);
  });

  test('button edges flush pending motion first', () async {
    c.move(const Offset(5, 0), ms(0));
    c.buttonDown(MouseButtonKind.left);
    await Future<void>.delayed(Duration.zero);
    c.move(const Offset(5, 0), ms(8));
    c.buttonUp(MouseButtonKind.left);
    await Future<void>.delayed(Duration.zero);
    expect(client.calls.map((x) => x.split(' ').first), [
      'pointer',
      'button',
      'pointer',
      'button',
    ]);
  });

  test('beginGesture flushes the previous gesture first', () {
    c.move(const Offset(5, 0), ms(0));
    c.beginGesture();
    expect(client.calls.map((x) => x.split(' ').first), [
      'pointer',
      'beginGesture',
    ]);
  });
}
