import 'dart:async';
import 'dart:ui';

import '../bridge/lanpilot_client.dart';
import '../connection/input_controller.dart';
import '../settings/settings.dart';
import 'gesture_machine.dart';
import 'pointer_math.dart';

/// Applies the pointer curve and scroll settings to gestures and sends the
/// result once per frame (spec 5.2, 5.3).
class TouchpadController implements GestureSink {
  TouchpadController({
    required this._input,
    required this._settings,
    required this._onHaptic,
  });

  final InputController _input;
  final Settings Function() _settings;
  final void Function(HapticKind) _onHaptic;
  final _speed = SpeedTracker();

  Offset _px = Offset.zero;
  Offset _notches = Offset.zero;
  double _zoomSteps = 0;
  Inertia? _inertia;
  Duration? _lastFrame;

  bool get needsFrames =>
      _inertia != null ||
      _px != Offset.zero ||
      _notches != Offset.zero ||
      _zoomSteps != 0;

  @override
  void beginGesture() {
    flush();
    _speed.reset();
    _input.beginGesture();
  }

  @override
  void move(Offset deltaPt, Duration time) {
    final s = _settings();
    _px += pointerPixels(
      deltaPt,
      _speed.update(deltaPt, time),
      sensitivity: s.sensitivity,
      preset: s.accel,
    );
  }

  @override
  void scroll(Offset deltaPt) {
    final s = _settings();
    _notches += scrollNotches(
      deltaPt,
      speed: s.scrollSpeed,
      natural: s.naturalScroll,
    );
  }

  @override
  void scrollEnd(Offset velocityPtPerSec) {
    final inertia = Inertia(velocityPtPerSec);
    _inertia = inertia.done ? null : inertia;
    _lastFrame = null;
  }

  @override
  void stopInertia() => _inertia = null;

  @override
  void zoom(double scaleRatio) => _zoomSteps += zoomSteps(scaleRatio);

  @override
  void click(MouseButtonKind button) {
    flush();
    unawaited(_input.click(button));
  }

  @override
  void buttonDown(MouseButtonKind button) {
    flush();
    unawaited(_input.button(button, down: true));
  }

  @override
  void buttonUp(MouseButtonKind button) {
    flush();
    unawaited(_input.button(button, down: false));
  }

  @override
  void haptic(HapticKind kind) {
    if (_settings().haptics) _onHaptic(kind);
  }

  /// Once per frame: advance inertia, then send what accumulated.
  void onFrame(Duration now) {
    final inertia = _inertia;
    final last = _lastFrame;
    _lastFrame = now;
    if (inertia != null && last != null && now > last) {
      scroll(inertia.step(now - last));
      if (inertia.done) _inertia = null;
    }
    flush();
  }

  /// The frame clock restarted (the ticker was stopped).
  void resetFrameClock() => _lastFrame = null;

  void flush() {
    if (_px != Offset.zero) {
      _input.pointer(_px.dx, _px.dy);
      _px = Offset.zero;
    }
    if (_notches != Offset.zero) {
      _input.scroll(_notches.dx, _notches.dy);
      _notches = Offset.zero;
    }
    if (_zoomSteps != 0) {
      final steps = _zoomSteps;
      _zoomSteps = 0;
      unawaited(_input.zoom(steps));
    }
  }
}
