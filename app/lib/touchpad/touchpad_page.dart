import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:wakelock_plus/wakelock_plus.dart';

import '../app/providers.dart';
import '../bridge/lanpilot_client.dart';
import '../connection/conn_state.dart';
import '../connection/connection_overlay.dart';
import '../connection/input_controller.dart';
import '../l10n/app_localizations.dart';
import '../settings/settings.dart';
import '../theme/app_theme.dart';
import 'gesture_machine.dart';
import 'touchpad_controller.dart';

void _haptic(HapticKind kind) => unawaited(
  kind == HapticKind.tap
      ? HapticFeedback.lightImpact()
      : HapticFeedback.mediumImpact(),
);

void _keepAwake(bool on) =>
    unawaited(WakelockPlus.toggle(enable: on).catchError((Object _) {}));

/// The touchpad (spec 2.2, 5): the pad, the optional left and right buttons,
/// and the connection overlay. Keeps the screen on while visible.
class TouchpadPage extends ConsumerStatefulWidget {
  const TouchpadPage({super.key});

  @override
  ConsumerState<TouchpadPage> createState() => _TouchpadPageState();
}

class _TouchpadPageState extends ConsumerState<TouchpadPage>
    with SingleTickerProviderStateMixin {
  late final _connection = ref.read(connectionProvider);
  late final _settings = ref.read(settingsProvider);
  late final TouchpadController _controller;
  late final GestureMachine _machine;
  late final Ticker _ticker;

  @override
  void initState() {
    super.initState();
    _controller = TouchpadController(
      input: _connection.input,
      settings: () => _settings.value,
      onHaptic: _haptic,
    );
    _machine = GestureMachine(_controller, config: _config());
    _settings.addListener(_reconfigure);
    _connection.state.addListener(_reconfigure);
    _ticker = createTicker(_onTick);
    _keepAwake(true);
  }

  @override
  void dispose() {
    _machine.dispose();
    _controller.flush();
    _settings.removeListener(_reconfigure);
    _connection.state.removeListener(_reconfigure);
    _ticker.dispose();
    _keepAwake(false);
    super.dispose();
  }

  GestureConfig _config() => GestureConfig(
    tapDelay: Duration(milliseconds: _settings.value.tapDelayMs),
    zoomEnabled:
        _connection.state.value.session?.capabilities.contains('zoom') ?? false,
  );

  void _reconfigure() => _machine.config = _config();

  void _onTick(Duration elapsed) {
    _controller.onFrame(elapsed);
    if (!_controller.needsFrames) _ticker.stop();
  }

  void _kick() {
    if (_ticker.isActive || !_controller.needsFrames) return;
    _controller.resetFrameClock();
    unawaited(_ticker.start());
  }

  @override
  Widget build(BuildContext context) {
    final p = context.palette;
    final landscape =
        MediaQuery.orientationOf(context) == Orientation.landscape;
    return ListenableBuilder(
      listenable: Listenable.merge([_settings, _connection.state]),
      builder: (context, _) {
        final Settings settings = _settings.value;
        final ConnState state = _connection.state.value;
        final radius = BorderRadius.circular(20);
        final pad = Expanded(
          child: Padding(
            padding: EdgeInsets.all(p.touchpadInset),
            child: Stack(
              children: [
                Positioned.fill(
                  child: Listener(
                    behavior: HitTestBehavior.opaque,
                    onPointerDown: (e) {
                      _machine.pointerDown(
                        e.pointer,
                        e.localPosition,
                        e.timeStamp,
                      );
                      _kick();
                    },
                    onPointerMove: (e) {
                      _machine.pointerMove(
                        e.pointer,
                        e.localPosition,
                        e.timeStamp,
                      );
                      _kick();
                    },
                    onPointerUp: (e) {
                      _machine.pointerUp(e.pointer, e.timeStamp);
                      _kick();
                    },
                    onPointerCancel: (e) {
                      _machine.pointerCancel(e.pointer, e.timeStamp);
                      _kick();
                    },
                    child: Frosted(
                      borderRadius: radius,
                      child: Container(
                        key: const Key('touchpad'),
                        decoration: BoxDecoration(
                          color: p.touchpadFill,
                          borderRadius: radius,
                          border: Border.all(color: p.touchpadBorder),
                        ),
                      ),
                    ),
                  ),
                ),
                Positioned.fill(child: ConnectionOverlay(state: state)),
              ],
            ),
          ),
        );
        final strip = settings.showButtonBar
            ? MouseButtonStrip(
                vertical: landscape,
                input: _connection.input,
                onEdge: (down) {
                  _controller.flush();
                  if (down) {
                    _controller.stopInertia();
                    _controller.haptic(HapticKind.tap);
                  }
                },
              )
            : null;
        if (landscape) {
          return Row(
            children: [
              pad,
              if (strip != null)
                Padding(
                  padding: EdgeInsets.fromLTRB(
                    0,
                    p.touchpadInset,
                    p.touchpadInset,
                    p.touchpadInset,
                  ),
                  child: strip,
                ),
            ],
          );
        }
        return Column(
          children: [
            pad,
            if (strip != null)
              Padding(
                padding: EdgeInsets.fromLTRB(
                  p.touchpadInset,
                  0,
                  p.touchpadInset,
                  p.touchpadInset,
                ),
                child: strip,
              ),
          ],
        );
      },
    );
  }
}

/// Left and right buttons: down when pressed, up when released, so they can
/// be held while dragging on the pad (spec 5.1).
class MouseButtonStrip extends StatelessWidget {
  const MouseButtonStrip({
    super.key,
    required this.vertical,
    required this.input,
    required this.onEdge,
  });

  final bool vertical;
  final InputController input;

  /// Called before each button edge to the computer.
  final void Function(bool down) onEdge;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final p = context.palette;
    Widget button(String label, MouseButtonKind kind, Key key) => Expanded(
      child: Listener(
        key: key,
        behavior: HitTestBehavior.opaque,
        onPointerDown: (_) {
          onEdge(true);
          unawaited(input.button(kind, down: true));
        },
        onPointerUp: (_) {
          onEdge(false);
          unawaited(input.button(kind, down: false));
        },
        onPointerCancel: (_) {
          onEdge(false);
          unawaited(input.button(kind, down: false));
        },
        child: Frosted(
          borderRadius: BorderRadius.circular(14),
          child: Container(
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: p.controlFill,
              borderRadius: BorderRadius.circular(14),
              border: Border.all(color: p.touchpadBorder),
            ),
            child: Text(label, style: TextStyle(color: p.text)),
          ),
        ),
      ),
    );
    final left = button(
      l.buttonLeft,
      MouseButtonKind.left,
      const Key('button-left'),
    );
    final right = button(
      l.buttonRight,
      MouseButtonKind.right,
      const Key('button-right'),
    );
    return vertical
        ? SizedBox(
            width: 88,
            child: Column(children: [left, const SizedBox(height: 12), right]),
          )
        : SizedBox(
            height: 64,
            child: Row(children: [left, const SizedBox(width: 12), right]),
          );
  }
}
