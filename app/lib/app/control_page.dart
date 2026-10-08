import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../connection/conn_state.dart';
import '../connection/status_header.dart';
import '../connection/switcher_sheet.dart';
import '../l10n/app_localizations.dart';
import '../media/media_page.dart';
import '../shortcuts/shortcuts_page.dart';
import '../touchpad/touchpad_page.dart';
import 'providers.dart';

/// Status header and the three tabs (spec 2.2). Portrait: tabs at the
/// bottom. Landscape: icons in a rail on the left.
class ControlPage extends ConsumerStatefulWidget {
  const ControlPage({super.key});

  @override
  ConsumerState<ControlPage> createState() => _ControlPageState();
}

class _ControlPageState extends ConsumerState<ControlPage> {
  late final _connection = ref.read(connectionProvider);
  int _tab = 0;

  @override
  void initState() {
    super.initState();
    _connection.state.addListener(_onState);
    WidgetsBinding.instance.addPostFrameCallback((_) => _onState());
  }

  @override
  void dispose() {
    _connection.state.removeListener(_onState);
    super.dispose();
  }

  void _onState() {
    if (!mounted) return;
    final s = _connection.state.value;
    if (s.showSwitcher) {
      _connection.acknowledgeSwitcher();
      unawaited(showSwitcher(context));
    }
    final noComputers = ref.read(serverStoreProvider).servers.value.isEmpty;
    if (s.status == ConnStatus.unpaired && noComputers) context.go('/add');
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final landscape =
        MediaQuery.orientationOf(context) == Orientation.landscape;
    final tabs = [
      (Icons.touch_app_outlined, l.tabTouchpad),
      (Icons.bolt_outlined, l.tabShortcuts),
      (Icons.music_note_outlined, l.tabMedia),
    ];
    final body = Column(
      children: [
        const StatusHeader(),
        Expanded(
          child: IndexedStack(
            index: _tab,
            children: const [TouchpadPage(), ShortcutsPage(), MediaPage()],
          ),
        ),
      ],
    );
    void select(int i) => setState(() => _tab = i);
    if (landscape) {
      return Scaffold(
        body: SafeArea(
          child: Row(
            children: [
              NavigationRail(
                selectedIndex: _tab,
                onDestinationSelected: select,
                labelType: NavigationRailLabelType.none,
                destinations: [
                  for (final (icon, label) in tabs)
                    NavigationRailDestination(
                      icon: Icon(icon),
                      label: Text(label),
                    ),
                ],
              ),
              Expanded(child: body),
            ],
          ),
        ),
      );
    }
    return Scaffold(
      body: SafeArea(bottom: false, child: body),
      bottomNavigationBar: NavigationBar(
        selectedIndex: _tab,
        onDestinationSelected: select,
        destinations: [
          for (final (icon, label) in tabs)
            NavigationDestination(icon: Icon(icon), label: label),
        ],
      ),
    );
  }
}
