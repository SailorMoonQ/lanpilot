import 'dart:math' as math;
import 'dart:ui';

import '../settings/settings.dart';
import 'tuning.dart';

/// Acceleration factor: about 1.0 when slow, rising smoothly with finger
/// speed up to the preset's cap (spec 5.2).
double accelGain(double speedPtPerSec, AccelPreset preset) {
  final cap = Tuning.accelMaxGain[preset]!;
  final t =
      ((speedPtPerSec - Tuning.accelSlowSpeed) /
              (Tuning.accelFastSpeed - Tuning.accelSlowSpeed))
          .clamp(0.0, 1.0);
  final smooth = t * t * (3 - 2 * t);
  return 1.0 + (cap - 1.0) * smooth;
}

Offset pointerPixels(
  Offset deltaPt,
  double speedPtPerSec, {
  required double sensitivity,
  required AccelPreset preset,
}) =>
    deltaPt *
    (Tuning.basePxPerPt * sensitivity * accelGain(speedPtPerSec, preset));

/// Two-finger translation to wheel notches. Positive y is wheel up (content
/// moves down), positive x is wheel right. Natural scrolling moves the content
/// with the fingers.
Offset scrollNotches(
  Offset deltaPt, {
  required double speed,
  required bool natural,
}) {
  final n = deltaPt / Tuning.scrollPtPerNotch * speed;
  return natural ? Offset(-n.dx, n.dy) : Offset(n.dx, -n.dy);
}

/// Pinch spread ratio to zoom steps; positive zooms in.
double zoomSteps(double scaleRatio) =>
    math.log(scaleRatio) / math.ln2 * Tuning.zoomStepsPerDoubling;

/// Smoothed finger speed in pt/s.
class SpeedTracker {
  double _speed = 0;
  Duration? _last;

  double update(Offset deltaPt, Duration time) {
    final last = _last;
    _last = time;
    if (last == null) return _speed = 0;
    var dt = time - last;
    if (dt < Tuning.minSampleInterval) dt = Tuning.minSampleInterval;
    final instant = deltaPt.distance / (dt.inMicroseconds / 1e6);
    const a = Tuning.speedSmoothing;
    return _speed = _speed * (1 - a) + instant * a;
  }

  void reset() {
    _speed = 0;
    _last = null;
  }
}

/// Recent positions, for the fling velocity when the fingers lift.
class MotionSampler {
  final _samples = <(Offset, Duration)>[];

  void reset() => _samples.clear();

  void add(Offset position, Duration time) {
    _samples.add((position, time));
    final cutoff = time - Tuning.velocityWindow;
    _samples.removeWhere((s) => s.$2 < cutoff);
  }

  /// Velocity in pt/s at `now`; zero if the fingers rested before lifting.
  Offset velocityAt(Duration now) {
    if (_samples.length < 2) return Offset.zero;
    final (p0, t0) = _samples.first;
    final (p1, t1) = _samples.last;
    if (now - t1 > Tuning.velocityWindow) return Offset.zero;
    final dt = (t1 - t0).inMicroseconds / 1e6;
    return dt <= 0 ? Offset.zero : (p1 - p0) / dt;
  }
}

/// Exponentially decaying scroll after a fling (spec 5.1).
class Inertia {
  Inertia(Offset velocityPtPerSec) : _v = _cap(velocityPtPerSec);

  Offset _v;

  bool get done => _v.distance < Tuning.inertiaMinSpeed;

  /// Distance travelled during `dt`, in pt.
  Offset step(Duration dt) {
    if (done) return Offset.zero;
    final tau = Tuning.inertiaTimeConstant.inMicroseconds / 1e6;
    final decay = math.exp(-(dt.inMicroseconds / 1e6) / tau);
    final travelled = _v * (tau * (1 - decay));
    _v = _v * decay;
    return travelled;
  }

  static Offset _cap(Offset v) => v.distance > Tuning.inertiaMaxSpeed
      ? v / v.distance * Tuning.inertiaMaxSpeed
      : v;
}
