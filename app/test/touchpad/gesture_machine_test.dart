import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/touchpad/gesture_machine.dart';

Duration ms(int v) => Duration(milliseconds: v);

class RecordingSink implements GestureSink {
  final log = <String>[];

  /// The log without bookkeeping entries (begin, stop, haptics).
  List<String> get actions => log
      .where((e) => e != 'begin' && e != 'stop' && !e.startsWith('haptic'))
      .toList();

  @override
  void beginGesture() => log.add('begin');
  @override
  void move(Offset d, Duration time) =>
      log.add('move ${d.dx.round()},${d.dy.round()}');
  @override
  void scroll(Offset d) => log.add('scroll ${d.dx.round()},${d.dy.round()}');
  @override
  void scrollEnd(Offset v) => log.add('scrollEnd');
  @override
  void zoom(double r) => log.add('zoom ${r.toStringAsFixed(2)}');
  @override
  void click(MouseButtonKind b) => log.add('click ${b.name}');
  @override
  void buttonDown(MouseButtonKind b) => log.add('down ${b.name}');
  @override
  void buttonUp(MouseButtonKind b) => log.add('up ${b.name}');
  @override
  void stopInertia() => log.add('stop');
  @override
  void haptic(HapticKind k) => log.add('haptic ${k.name}');
}

void main() {
  late RecordingSink sink;
  late GestureMachine m;

  void setUpMachine({bool zoom = false, int tapDelayMs = 180}) {
    sink = RecordingSink();
    m = GestureMachine(
      sink,
      config: GestureConfig(tapDelay: ms(tapDelayMs), zoomEnabled: zoom),
    );
  }

  void tap(int id, int at, {Offset where = Offset.zero}) {
    m.pointerDown(id, where, ms(at));
    m.pointerUp(id, ms(at + 80));
  }

  test('single finger motion starts after the threshold', () {
    setUpMachine();
    m.pointerDown(1, Offset.zero, ms(0));
    m.pointerMove(1, const Offset(2, 0), ms(10));
    expect(sink.actions, isEmpty);
    m.pointerMove(1, const Offset(10, 0), ms(20));
    m.pointerMove(1, const Offset(15, 0), ms(30));
    m.pointerUp(1, ms(400));
    expect(sink.actions, ['move 10,0', 'move 5,0']);
    expect(sink.log.take(2), ['stop', 'begin']);
  });

  test('a quick tap clicks after the tap delay', () {
    fakeAsync((async) {
      setUpMachine();
      tap(1, 0);
      async.elapse(ms(179));
      expect(sink.actions, isEmpty);
      async.elapse(ms(2));
      expect(sink.actions, ['click left']);
      expect(sink.log, contains('haptic tap'));
    });
  });

  test('zero tap delay clicks immediately', () {
    setUpMachine(tapDelayMs: 0);
    tap(1, 0);
    expect(sink.actions, ['click left']);
  });

  test('a slow press is not a tap', () {
    fakeAsync((async) {
      setUpMachine();
      m.pointerDown(1, Offset.zero, ms(0));
      m.pointerUp(1, ms(250));
      async.elapse(ms(500));
      expect(sink.actions, isEmpty);
    });
  });

  test('a tap with a small wobble still clicks', () {
    fakeAsync((async) {
      setUpMachine();
      m.pointerDown(1, Offset.zero, ms(0));
      m.pointerMove(1, const Offset(5, 0), ms(30));
      m.pointerUp(1, ms(80));
      async.elapse(ms(200));
      expect(sink.actions, ['move 5,0', 'click left']);
    });
  });

  test('double tap is a double click right away', () {
    fakeAsync((async) {
      setUpMachine();
      tap(1, 0);
      async.elapse(ms(50));
      tap(2, 130);
      expect(sink.actions, ['click left', 'click left']);
      async.elapse(ms(500));
      expect(sink.actions, ['click left', 'click left']);
    });
  });

  test('tap then hold and move drags', () {
    fakeAsync((async) {
      setUpMachine();
      tap(1, 0);
      async.elapse(ms(50));
      m.pointerDown(2, Offset.zero, ms(130));
      m.pointerMove(2, const Offset(10, 0), ms(150));
      m.pointerMove(2, const Offset(20, 0), ms(160));
      m.pointerUp(2, ms(600));
      async.elapse(ms(500));
      expect(sink.actions, ['down left', 'move 10,0', 'move 10,0', 'up left']);
      expect(sink.log, contains('haptic dragStart'));
    });
  });

  test('cancel during a drag releases the button', () {
    fakeAsync((async) {
      setUpMachine();
      tap(1, 0);
      async.elapse(ms(50));
      m.pointerDown(2, Offset.zero, ms(130));
      m.pointerMove(2, const Offset(10, 0), ms(150));
      m.pointerCancel(2, ms(170));
      async.elapse(ms(500));
      expect(sink.actions, ['down left', 'move 10,0', 'up left']);
    });
  });

  test('cancel before lifting never clicks', () {
    fakeAsync((async) {
      setUpMachine();
      m.pointerDown(1, Offset.zero, ms(0));
      m.pointerCancel(1, ms(50));
      async.elapse(ms(500));
      expect(sink.actions, isEmpty);
    });
  });

  test('two-finger tap is a right click', () {
    setUpMachine();
    m.pointerDown(1, Offset.zero, ms(0));
    m.pointerDown(2, const Offset(50, 0), ms(30));
    m.pointerUp(1, ms(120));
    m.pointerUp(2, ms(140));
    expect(sink.actions, ['click right']);
  });

  test('three-finger tap is a middle click', () {
    setUpMachine();
    m.pointerDown(1, Offset.zero, ms(0));
    m.pointerDown(2, const Offset(40, 0), ms(20));
    m.pointerDown(3, const Offset(80, 0), ms(40));
    m.pointerUp(1, ms(120));
    m.pointerUp(2, ms(130));
    m.pointerUp(3, ms(140));
    expect(sink.actions, ['click middle']);
  });

  test('fingers landing far apart in time are not a tap', () {
    setUpMachine();
    m.pointerDown(1, Offset.zero, ms(0));
    m.pointerDown(2, const Offset(50, 0), ms(150));
    m.pointerUp(1, ms(200));
    m.pointerUp(2, ms(210));
    expect(sink.actions, isEmpty);
  });

  test('a long two-finger press is not a tap', () {
    setUpMachine();
    m.pointerDown(1, Offset.zero, ms(0));
    m.pointerDown(2, const Offset(50, 0), ms(20));
    m.pointerUp(1, ms(390));
    m.pointerUp(2, ms(400));
    expect(sink.actions, isEmpty);
  });

  test('two-finger drag scrolls and flings, never moves', () {
    setUpMachine();
    m.pointerDown(1, Offset.zero, ms(0));
    m.pointerDown(2, const Offset(40, 0), ms(10));
    for (var i = 1; i <= 5; i++) {
      m.pointerMove(1, Offset(0, i * 10.0), ms(10 + i * 10));
      m.pointerMove(2, Offset(40, i * 10.0), ms(10 + i * 10));
    }
    m.pointerUp(1, ms(70));
    m.pointerUp(2, ms(75));
    final actions = sink.actions;
    expect(actions.first, startsWith('scroll'));
    expect(actions.last, 'scrollEnd');
    expect(actions.where((a) => a == 'scrollEnd'), hasLength(1));
    expect(actions.any((a) => a.startsWith('move')), isFalse);
    expect(actions.any((a) => a.startsWith('click')), isFalse);
  });

  test(
    'lifting one scrolling finger ends the scroll; the other is ignored',
    () {
      setUpMachine();
      m.pointerDown(1, Offset.zero, ms(0));
      m.pointerDown(2, const Offset(40, 0), ms(10));
      m.pointerMove(1, const Offset(0, 20), ms(30));
      m.pointerMove(2, const Offset(40, 20), ms(30));
      m.pointerUp(1, ms(40));
      final count = sink.actions.length;
      m.pointerMove(2, const Offset(80, 80), ms(60));
      m.pointerUp(2, ms(80));
      expect(sink.actions, hasLength(count));
      expect(sink.actions.last, 'scrollEnd');
    },
  );

  test('pinch zooms only when the computer supports it', () {
    setUpMachine(zoom: true);
    m.pointerDown(1, Offset.zero, ms(0));
    m.pointerDown(2, const Offset(100, 0), ms(10));
    m.pointerMove(1, const Offset(-20, 0), ms(30));
    m.pointerMove(2, const Offset(120, 0), ms(30));
    // 100 -> 120 -> 140 pt apart: ratios since the previous call.
    expect(sink.actions.where((a) => a.startsWith('zoom')), [
      'zoom 1.20',
      'zoom 1.17',
    ]);

    setUpMachine();
    m.pointerDown(1, Offset.zero, ms(0));
    m.pointerDown(2, const Offset(100, 0), ms(10));
    m.pointerMove(1, const Offset(-20, 0), ms(30));
    m.pointerMove(2, const Offset(120, 0), ms(30));
    expect(sink.actions.where((a) => a.startsWith('zoom')), isEmpty);
  });

  test('a second finger right after a tap: tap clicks, then right click', () {
    fakeAsync((async) {
      setUpMachine();
      tap(1, 0);
      async.elapse(ms(40));
      m.pointerDown(2, Offset.zero, ms(130));
      m.pointerDown(3, const Offset(50, 0), ms(150));
      m.pointerUp(2, ms(200));
      m.pointerUp(3, ms(210));
      async.elapse(ms(500));
      expect(sink.actions, ['click left', 'click right']);
    });
  });

  test('every new touch stops inertia', () {
    setUpMachine();
    m.pointerDown(1, Offset.zero, ms(0));
    m.pointerUp(1, ms(400));
    m.pointerDown(2, Offset.zero, ms(500));
    expect(sink.log.where((e) => e == 'stop'), hasLength(2));
  });

  test('dispose releases a drag', () {
    fakeAsync((async) {
      setUpMachine();
      tap(1, 0);
      async.elapse(ms(50));
      m.pointerDown(2, Offset.zero, ms(130));
      m.pointerMove(2, const Offset(10, 0), ms(150));
      m.dispose();
      expect(sink.actions.last, 'up left');
    });
  });

  void startDrag(FakeAsync async) {
    tap(1, 0);
    async.elapse(ms(50));
    m.pointerDown(2, Offset.zero, ms(130));
    m.pointerMove(2, const Offset(10, 0), ms(150));
  }

  test('cancelling a resting extra finger then the primary releases once', () {
    fakeAsync((async) {
      setUpMachine();
      startDrag(async);
      m.pointerDown(3, const Offset(50, 0), ms(160));
      m.pointerCancel(3, ms(170));
      m.pointerCancel(2, ms(180));
      async.elapse(ms(500));
      expect(sink.actions.where((a) => a == 'up left'), hasLength(1));
      expect(sink.actions.last, 'up left');
    });
  });

  test('a drag continues after an extra finger is cancelled', () {
    fakeAsync((async) {
      setUpMachine();
      startDrag(async);
      m.pointerDown(3, const Offset(50, 0), ms(160));
      m.pointerCancel(3, ms(170));
      m.pointerMove(2, const Offset(20, 0), ms(180));
      m.pointerUp(2, ms(600));
      expect(sink.actions, ['down left', 'move 10,0', 'move 10,0', 'up left']);
    });
  });

  test('dispose releases a drag with a resting extra finger', () {
    fakeAsync((async) {
      setUpMachine();
      startDrag(async);
      m.pointerDown(3, const Offset(50, 0), ms(160));
      m.dispose();
      expect(sink.actions.last, 'up left');
    });
  });

  test('a wobbly second tap is still a double click', () {
    fakeAsync((async) {
      setUpMachine();
      tap(1, 0);
      async.elapse(ms(50));
      m.pointerDown(2, Offset.zero, ms(130));
      m.pointerMove(2, const Offset(5, 0), ms(150));
      m.pointerUp(2, ms(210));
      async.elapse(ms(500));
      expect(sink.actions, ['down left', 'move 5,0', 'up left', 'click left']);
    });
  });

  test('a new touch after a scroll stops the fling', () {
    setUpMachine();
    m.pointerDown(1, Offset.zero, ms(0));
    m.pointerDown(2, const Offset(40, 0), ms(10));
    m.pointerMove(1, const Offset(0, 20), ms(30));
    m.pointerMove(2, const Offset(40, 20), ms(30));
    m.pointerUp(1, ms(40));
    m.pointerDown(3, const Offset(90, 0), ms(60));
    expect(sink.log.sublist(sink.log.indexOf('scrollEnd')), contains('stop'));
  });

  test('a brief second touch does not freeze the pointer', () {
    setUpMachine();
    m.pointerDown(1, Offset.zero, ms(0));
    m.pointerMove(1, const Offset(10, 0), ms(20));
    m.pointerDown(2, const Offset(100, 0), ms(100));
    m.pointerUp(2, ms(150));
    m.pointerMove(1, const Offset(20, 0), ms(170));
    expect(sink.actions, ['move 10,0', 'move 10,0']);
  });

  test('a staggered two-finger tap does not left click', () {
    fakeAsync((async) {
      setUpMachine();
      m.pointerDown(1, Offset.zero, ms(0));
      m.pointerDown(2, const Offset(50, 0), ms(120));
      m.pointerUp(1, ms(160));
      m.pointerUp(2, ms(200));
      async.elapse(ms(500));
      expect(sink.actions, isEmpty);
    });
  });

  test('a three-finger tap with one sliding finger does not left click', () {
    fakeAsync((async) {
      setUpMachine();
      m.pointerDown(1, Offset.zero, ms(0));
      m.pointerDown(2, const Offset(40, 0), ms(20));
      m.pointerDown(3, const Offset(80, 0), ms(40));
      m.pointerMove(2, const Offset(40, 10), ms(60));
      m.pointerUp(2, ms(90));
      m.pointerUp(1, ms(100));
      m.pointerUp(3, ms(150));
      async.elapse(ms(500));
      expect(sink.actions, isEmpty);
    });
  });

  test(
    'resting two fingers then lifting one leaves the other free to move',
    () {
      fakeAsync((async) {
        setUpMachine();
        m.pointerDown(1, Offset.zero, ms(0));
        m.pointerDown(2, const Offset(50, 0), ms(10));
        m.pointerUp(1, ms(400));
        m.pointerMove(2, const Offset(60, 0), ms(420));
        m.pointerUp(2, ms(900));
        async.elapse(ms(500));
        expect(sink.actions, ['move 10,0']);
      });
    },
  );
}
