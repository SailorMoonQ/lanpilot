import 'dart:async';
import 'dart:math' as math;
import 'dart:ui';

import '../bridge/lanpilot_client.dart' show MouseButtonKind;
import 'pointer_math.dart';
import 'tuning.dart';

enum HapticKind { tap, dragStart }

/// What the gesture machine asks for. Distances are in points.
abstract interface class GestureSink {
  void beginGesture();

  void move(Offset deltaPt, Duration time);

  void scroll(Offset deltaPt);

  void scrollEnd(Offset velocityPtPerSec);

  /// Spread ratio since the previous zoom call (> 1 spreads apart).
  void zoom(double scaleRatio);

  void click(MouseButtonKind button);

  void buttonDown(MouseButtonKind button);

  void buttonUp(MouseButtonKind button);

  void stopInertia();

  void haptic(HapticKind kind);
}

class GestureConfig {
  const GestureConfig({
    this.tapDelay = const Duration(milliseconds: 180),
    this.zoomEnabled = false,
  });

  final Duration tapDelay;

  /// Pinch is recognized only when the computer has the `zoom` capability.
  final bool zoomEnabled;
}

enum _Phase {
  idle,
  touch,
  moving,
  tapWait,
  dragArmed,
  dragging,
  multi,
  scrolling,
  pinching,
  done,
}

class _Finger {
  _Finger(this.start, this.downAt) : last = start;

  final Offset start;
  final Duration downAt;
  Offset last;
  double travel = 0;
}

/// Turns raw touches into pointer actions (spec 5.1). Pure Dart: feed it
/// pointer events and it calls [GestureSink]. The tap delay uses a [Timer].
class GestureMachine {
  GestureMachine(this._sink, {this.config = const GestureConfig()});

  final GestureSink _sink;
  GestureConfig config;

  final _fingers = <int, _Finger>{};
  final _sampler = MotionSampler();
  _Phase _phase = _Phase.idle;
  Timer? _tapTimer;
  int? _primary;
  Duration _firstDown = Duration.zero;
  int _maxFingers = 0;
  bool _tapPossible = false;
  bool _leftHeld = false;
  List<int> _pair = const [];
  Offset _pairStartCentroid = Offset.zero;
  double _pairStartDistance = 0;
  Offset _lastCentroid = Offset.zero;
  double _lastDistance = 0;

  void pointerDown(int id, Offset position, Duration time) {
    _sink.stopInertia();
    _fingers[id] = _Finger(position, time);
    switch (_phase) {
      case _Phase.idle:
        _startGesture(id, time);
        _phase = _Phase.touch;
      case _Phase.tapWait:
        _tapTimer?.cancel();
        _startGesture(id, time);
        _phase = _Phase.dragArmed;
      case _Phase.touch:
        _enterMulti(tapPossible: time - _firstDown <= Tuning.multiDownWindow);
      case _Phase.dragArmed:
        // A second finger right after a tap: that tap was a plain click.
        _clickNow(MouseButtonKind.left);
        _enterMulti(tapPossible: time - _firstDown <= Tuning.multiDownWindow);
      case _Phase.moving:
        _enterMulti(tapPossible: false);
      case _Phase.multi:
        _maxFingers = math.max(_maxFingers, _fingers.length);
        if (time - _firstDown > Tuning.multiDownWindow) _tapPossible = false;
        _choosePair();
      case _Phase.dragging ||
          _Phase.scrolling ||
          _Phase.pinching ||
          _Phase.done:
        break; // Extra fingers never change a running gesture.
    }
  }

  void pointerMove(int id, Offset position, Duration time) {
    final f = _fingers[id];
    if (f == null) return;
    final delta = position - f.last;
    f.last = position;
    f.travel = math.max(f.travel, (position - f.start).distance);
    switch (_phase) {
      case _Phase.touch:
        if (f.travel > Tuning.moveThreshold) {
          _phase = _Phase.moving;
          _sink.move(position - f.start, time);
        }
      case _Phase.moving || _Phase.dragging:
        if (id == _primary) _sink.move(delta, time);
      case _Phase.dragArmed:
        if (f.travel > Tuning.moveThreshold) {
          _phase = _Phase.dragging;
          _leftHeld = true;
          _sink.buttonDown(MouseButtonKind.left);
          _sink.haptic(HapticKind.dragStart);
          _sink.move(position - f.start, time);
        }
      case _Phase.multi:
        _multiMove(time);
      case _Phase.scrolling:
        if (_pair.contains(id)) {
          final c = _centroid();
          _sink.scroll(c - _lastCentroid);
          _lastCentroid = c;
          _sampler.add(c, time);
        }
      case _Phase.pinching:
        if (_pair.contains(id)) {
          final d = _distance();
          if (d > 0 && _lastDistance > 0) _sink.zoom(d / _lastDistance);
          _lastDistance = d;
        }
      case _Phase.idle || _Phase.tapWait || _Phase.done:
        break;
    }
  }

  void pointerUp(int id, Duration time) {
    final f = _fingers.remove(id);
    if (f == null) return;
    final quick =
        time - f.downAt < Tuning.tapMaxDuration && f.travel < Tuning.tapMaxMove;
    switch (_phase) {
      case _Phase.touch || _Phase.moving:
        if (quick) {
          _startTapWait();
        } else {
          _phase = _Phase.idle;
        }
      case _Phase.dragArmed:
        _clickNow(MouseButtonKind.left); // the first tap
        if (quick) _clickNow(MouseButtonKind.left); // and this one
        _phase = _Phase.idle;
      case _Phase.dragging:
        if (id == _primary) {
          _releaseLeft();
          // A wobbly second tap of a double tap: the first tap still clicks.
          if (quick) _clickNow(MouseButtonKind.left);
          _phase = _rest();
        }
      case _Phase.multi:
        if (f.travel >= Tuning.tapMaxMove) _tapPossible = false;
        if (_fingers.length == 1 && !_tapPossible) {
          // A brief extra touch ended: the remaining finger moves the pointer.
          _primary = _fingers.keys.first;
          _phase = _Phase.moving;
          return;
        }
        if (_fingers.isNotEmpty) {
          _choosePair();
          return;
        }
        final tap = _tapPossible && time - _firstDown <= Tuning.multiUpWindow;
        if (tap && _maxFingers == 2) _clickNow(MouseButtonKind.right);
        if (tap && _maxFingers == 3) _clickNow(MouseButtonKind.middle);
        _phase = _Phase.idle;
      case _Phase.scrolling:
        if (_pair.contains(id)) {
          _sink.scrollEnd(_sampler.velocityAt(time));
          _phase = _rest();
        }
      case _Phase.pinching:
        if (_pair.contains(id)) _phase = _rest();
      case _Phase.done:
        _phase = _rest();
      case _Phase.idle || _Phase.tapWait:
        break;
    }
  }

  /// The system took the touch (a call, Control Center). The cancelled touch
  /// itself never clicks (a tap already completed before it still does) and a
  /// held drag is always released. A cancelled scroll does not fling.
  void pointerCancel(int id, Duration time) {
    if (_fingers.remove(id) == null) return;
    if (_phase == _Phase.dragging && id != _primary) return;
    if (id == _primary) _releaseLeft();
    if (_phase == _Phase.dragArmed) _clickNow(MouseButtonKind.left);
    _tapPossible = false;
    _phase = _rest();
  }

  void dispose() {
    _tapTimer?.cancel();
    _releaseLeft();
    _fingers.clear();
    _phase = _Phase.idle;
  }

  void _releaseLeft() {
    if (!_leftHeld) return;
    _leftHeld = false;
    _sink.buttonUp(MouseButtonKind.left);
  }

  void _startGesture(int id, Duration time) {
    _sink.beginGesture();
    _primary = id;
    _firstDown = time;
    _maxFingers = 1;
    _tapPossible = false;
  }

  void _enterMulti({required bool tapPossible}) {
    _phase = _Phase.multi;
    _tapPossible = tapPossible;
    _maxFingers = math.max(_maxFingers, _fingers.length);
    _choosePair();
  }

  void _choosePair() {
    _pair = _fingers.keys.take(2).toList();
    _pairStartCentroid = _centroid();
    _pairStartDistance = _distance();
    _lastCentroid = _pairStartCentroid;
    _lastDistance = _pairStartDistance;
  }

  void _multiMove(Duration time) {
    if (_fingers.values.any((f) => f.travel >= Tuning.tapMaxMove)) {
      _tapPossible = false;
    }
    if (_fingers.length != 2 || _pair.length != 2) return;
    final c = _centroid();
    final d = _distance();
    final translation = (c - _pairStartCentroid).distance;
    final spread = (d - _pairStartDistance).abs();
    if (config.zoomEnabled &&
        spread > Tuning.pinchThreshold &&
        spread > translation) {
      _phase = _Phase.pinching;
      _tapPossible = false;
      if (_pairStartDistance > 0) _sink.zoom(d / _pairStartDistance);
      _lastDistance = d;
    } else if (translation > Tuning.moveThreshold &&
        (!config.zoomEnabled || translation >= spread)) {
      _phase = _Phase.scrolling;
      _tapPossible = false;
      _sampler
        ..reset()
        ..add(c, time);
      _sink.scroll(c - _pairStartCentroid);
      _lastCentroid = c;
    }
  }

  void _startTapWait() {
    if (config.tapDelay <= Duration.zero) {
      _clickNow(MouseButtonKind.left);
      _phase = _Phase.idle;
      return;
    }
    _phase = _Phase.tapWait;
    _tapTimer = Timer(config.tapDelay, () {
      _phase = _Phase.idle;
      _clickNow(MouseButtonKind.left);
    });
  }

  void _clickNow(MouseButtonKind button) {
    _sink.click(button);
    _sink.haptic(HapticKind.tap);
  }

  _Phase _rest() => _fingers.isEmpty ? _Phase.idle : _Phase.done;

  Offset _centroid() {
    if (_pair.isEmpty) return Offset.zero;
    var sum = Offset.zero;
    for (final id in _pair) {
      sum += _fingers[id]!.last;
    }
    return sum / _pair.length.toDouble();
  }

  double _distance() => _pair.length < 2
      ? 0
      : (_fingers[_pair[0]]!.last - _fingers[_pair[1]]!.last).distance;
}
