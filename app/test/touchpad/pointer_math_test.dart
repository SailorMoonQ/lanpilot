import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/settings/settings.dart';
import 'package:lanpilot/touchpad/pointer_math.dart';
import 'package:lanpilot/touchpad/tuning.dart';

Duration ms(int v) => Duration(milliseconds: v);

void main() {
  group('accelGain', () {
    test('is 1 when slow and capped when fast', () {
      for (final p in AccelPreset.values) {
        expect(accelGain(0, p), 1.0);
        expect(accelGain(1e6, p), Tuning.accelMaxGain[p]);
      }
      expect(Tuning.accelMaxGain[AccelPreset.high], 2.5);
      expect(accelGain(1e6, AccelPreset.off), 1.0);
    });

    test('rises smoothly with speed', () {
      var last = 0.0;
      for (var speed = 0.0; speed <= 2000; speed += 50) {
        final g = accelGain(speed, AccelPreset.medium);
        expect(g, greaterThanOrEqualTo(last));
        last = g;
      }
    });
  });

  test('pointerPixels scales by base, sensitivity and gain', () {
    final px = pointerPixels(
      const Offset(10, -5),
      0,
      sensitivity: 2,
      preset: AccelPreset.medium,
    );
    expect(px.dx, closeTo(10 * Tuning.basePxPerPt * 2, 1e-9));
    expect(px.dy, closeTo(-5 * Tuning.basePxPerPt * 2, 1e-9));
  });

  group('scrollNotches', () {
    test('natural: fingers down is wheel up, fingers left is wheel right', () {
      final n = scrollNotches(const Offset(-40, 40), speed: 1, natural: true);
      expect(n, const Offset(1, 1));
    });

    test('traditional flips both axes', () {
      final n = scrollNotches(const Offset(-40, 40), speed: 1, natural: false);
      expect(n, const Offset(-1, -1));
    });

    test('speed scales', () {
      expect(scrollNotches(const Offset(0, 40), speed: 2, natural: true).dy, 2);
    });
  });

  test('zoomSteps: doubling the spread zooms in by the configured steps', () {
    expect(zoomSteps(2), closeTo(Tuning.zoomStepsPerDoubling, 1e-9));
    expect(zoomSteps(0.5), closeTo(-Tuning.zoomStepsPerDoubling, 1e-9));
    expect(zoomSteps(1), 0);
  });

  test('zoomSteps: a degenerate ratio is no zoom', () {
    for (final ratio in [0.0, -1.0, double.nan, double.infinity]) {
      expect(zoomSteps(ratio), 0, reason: '$ratio');
    }
  });

  test('SpeedTracker smooths instant speed', () {
    final t = SpeedTracker();
    expect(t.update(const Offset(1, 0), ms(0)), 0);
    final s = t.update(const Offset(10, 0), ms(10));
    expect(s, greaterThan(0));
    expect(s, lessThanOrEqualTo(1000));
    t.reset();
    expect(t.update(const Offset(10, 0), ms(20)), 0);
  });

  group('MotionSampler', () {
    test('velocity over the recent window', () {
      final s = MotionSampler()
        ..add(Offset.zero, ms(0))
        ..add(const Offset(0, 50), ms(50));
      expect(s.velocityAt(ms(50)).dy, closeTo(1000, 1e-6));
    });

    test('a pause before lifting means no fling', () {
      final s = MotionSampler()
        ..add(Offset.zero, ms(0))
        ..add(const Offset(0, 50), ms(50));
      expect(s.velocityAt(ms(400)), Offset.zero);
    });
  });

  group('Inertia', () {
    test('travels and comes to rest', () {
      final i = Inertia(const Offset(0, 1000));
      var total = Offset.zero;
      var frames = 0;
      while (!i.done && frames < 1000) {
        total += i.step(ms(8));
        frames++;
      }
      expect(i.done, isTrue);
      final seconds = Tuning.inertiaTimeConstant.inMicroseconds / 1e6;
      expect(total.dy, closeTo(1000 * seconds, 1000 * seconds * 0.1));
      expect(i.step(ms(8)), Offset.zero);
    });

    test('slow flicks do not coast', () {
      expect(Inertia(const Offset(5, 5)).done, isTrue);
    });

    test('speed is capped', () {
      final i = Inertia(const Offset(1e9, 0));
      expect(i.step(ms(1)).dx, lessThan(Tuning.inertiaMaxSpeed / 1000 + 1));
    });
  });
}
