import 'package:go_router/go_router.dart';

import '../bridge/lanpilot_client.dart';
import '../pairing/add_computer_page.dart';
import '../pairing/password_pair_page.dart';
import '../pairing/scan_page.dart';
import '../settings/settings_page.dart';
import 'control_page.dart';

/// Start on the control page when a computer is paired, else on "Add
/// computer" (spec 2.3).
GoRouter buildRouter({required bool hasServers, String? initialLocation}) =>
    GoRouter(
      initialLocation: initialLocation ?? (hasServers ? '/control' : '/add'),
      routes: [
        GoRoute(path: '/control', builder: (_, _) => const ControlPage()),
        GoRoute(
          path: '/add',
          builder: (_, _) => const AddComputerPage(),
          routes: [
            GoRoute(path: 'scan', builder: (_, _) => const ScanPage()),
            GoRoute(
              path: 'manual',
              builder: (_, _) => const PasswordPairPage(),
            ),
            GoRoute(
              path: 'nearby',
              builder: (_, state) =>
                  PasswordPairPage(device: state.extra as DiscoveredInfo?),
            ),
          ],
        ),
        GoRoute(path: '/settings', builder: (_, _) => const SettingsPage()),
      ],
    );
