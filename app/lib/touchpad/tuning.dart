import '../settings/settings.dart';

/// Every touchpad feel constant (spec 5.1, 5.2). Initial values; tune them on
/// a real iPhone with docs/e2e/m2-checklist.md.
abstract final class Tuning {
  // Gesture recognition.
  static const moveThreshold = 3.0; // pt before a touch becomes motion
  static const tapMaxDuration = Duration(milliseconds: 200);
  static const tapMaxMove = 8.0; // pt
  static const multiDownWindow = Duration(milliseconds: 100);
  static const multiUpWindow = Duration(milliseconds: 250);
  static const pinchThreshold = 12.0; // pt of spread change before zooming

  // Pointer curve: px = pt * basePxPerPt * sensitivity * gain(speed).
  static const basePxPerPt = 1.6;
  static const accelSlowSpeed = 150.0; // pt/s, gain starts rising
  static const accelFastSpeed = 1500.0; // pt/s, gain reaches its cap
  static const accelMaxGain = {
    AccelPreset.off: 1.0,
    AccelPreset.low: 1.6,
    AccelPreset.medium: 2.0,
    AccelPreset.high: 2.5,
  };
  static const speedSmoothing = 0.5; // weight of the newest sample
  static const minSampleInterval = Duration(milliseconds: 4);

  // Scrolling and zoom.
  static const scrollPtPerNotch = 40.0;
  static const zoomStepsPerDoubling = 4.0;

  // Inertial scrolling.
  static const velocityWindow = Duration(milliseconds: 80);
  static const inertiaTimeConstant = Duration(milliseconds: 325);
  static const inertiaMinSpeed = 30.0; // pt/s
  static const inertiaMaxSpeed = 4000.0; // pt/s
}
