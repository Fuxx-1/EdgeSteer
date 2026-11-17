/// EdgeSteer configuration interface.
///
/// This process owns the window only. Closing it ends this process and nothing
/// else: the menu-bar agent keeps the Rust DNS engine running. Every operation
/// goes through the agent's authenticated control protocol, so the window never
/// touches the resolver, system DNS, or the configuration file directly.
library;

import 'dart:io';

import 'package:flutter/material.dart';

import 'agent_client.dart';
import 'app_state.dart';
import 'pages/cloudflare_page.dart';
import 'pages/json_page.dart';
import 'pages/overview_page.dart';
import 'pages/resolver_page.dart';
import 'pages/rules_page.dart';
import 'strings.dart';
import 'theme.dart';
import 'window_controls.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const EdgeSteerApp());
}

class EdgeSteerApp extends StatefulWidget {
  const EdgeSteerApp({super.key});

  @override
  State<EdgeSteerApp> createState() => _EdgeSteerAppState();
}

class _EdgeSteerAppState extends State<EdgeSteerApp> {
  final EdgeSteerController _controller = EdgeSteerController();

  @override
  void initState() {
    super.initState();
    _controller.initialise();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _controller,
      builder: (context, _) {
        final appearance = _controller.preferences.appearanceMode;
        return MaterialApp(
          title: 'EdgeSteer',
          debugShowCheckedModeBanner: false,
          // `system` resolves against the platform brightness. Driving this
          // through `themeMode` (instead of picking one theme ourselves) lets
          // MaterialApp follow the OS when it changes while we are running.
          theme: buildTheme(dark: false),
          darkTheme: buildTheme(dark: true),
          themeMode: switch (appearance) {
            AppearanceMode.system => ThemeMode.system,
            AppearanceMode.dark => ThemeMode.dark,
            AppearanceMode.light => ThemeMode.light,
          },
          home: EdgeSteerScope(
            controller: _controller,
            child: EdgeSteerShell(strings: _controller.strings),
          ),
        );
      },
    );
  }
}

/// Top-level sections. Each is one click from anywhere: the document editors are
/// siblings rather than a sub-menu inside a configuration section, because a
/// two-level tab bar made the common editing path cost an extra click.
enum _Section { status, resolvers, ruleSets, preferred, json }

class EdgeSteerShell extends StatefulWidget {
  const EdgeSteerShell({super.key, required this.strings});

  final Strings strings;

  @override
  State<EdgeSteerShell> createState() => _EdgeSteerShellState();
}

class _EdgeSteerShellState extends State<EdgeSteerShell> {
  late _Section _section;
  bool _noticeShown = false;

  @override
  void initState() {
    super.initState();
    _section = _sectionFor(_requestedPage());
  }

  /// `EDGESTEER_UI_PAGE=<name>` opens a specific page. Used by verification
  /// scripts and by agents driving the packaged interface; normal launches open
  /// the status page.
  static String? _requestedPage() =>
      Platform.environment['EDGESTEER_UI_PAGE'];

  static _Section _sectionFor(String? requested) => switch (requested) {
        'resolvers' => _Section.resolvers,
        'ruleSets' => _Section.ruleSets,
        'preferred' => _Section.preferred,
        'json' => _Section.json,
        // `config` and `system` predate the flat tabs. Keep accepting them so
        // existing scripts and menu-bar shortcuts still land somewhere useful.
        'config' => _Section.resolvers,
        'system' => _Section.status,
        _ => _Section.status,
      };

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final controller = EdgeSteerScope.of(context);
    if (!_noticeShown && !controller.loading) {
      _noticeShown = true;
      final notice = controller.startupNotice;
      if (notice != null && notice.isNotEmpty) {
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted) {
            _showNotice(notice);
          }
        });
      }
    }
  }

  void _showNotice(String message) {
    if (!mounted || message.isEmpty) {
      return;
    }
    ScaffoldMessenger.of(context)
      ..clearSnackBars()
      ..showSnackBar(
        SnackBar(
          content: Text(message, style: const TextStyle(fontSize: 12.5)),
          duration: const Duration(seconds: 4),
        ),
      );
  }

  Widget _buildSection(Strings strings) {
    final notice = _showNotice;
    return switch (_section) {
      _Section.status => OverviewPage(
          strings: strings,
          onNotice: notice,
          onQuit: () => exit(0),
        ),
      _Section.resolvers => ResolverPage(strings: strings, onNotice: notice),
      _Section.ruleSets => RulesPage(strings: strings, onNotice: notice),
      _Section.preferred => CloudflarePage(strings: strings, onNotice: notice),
      _Section.json => JsonPage(strings: strings, onNotice: notice),
    };
  }

  @override
  Widget build(BuildContext context) {
    final controller = EdgeSteerScope.of(context);
    final strings = widget.strings;
    final scheme = Theme.of(context).colorScheme;

    return Scaffold(
      backgroundColor: scheme.surface,
      body: Column(
        children: [
          _TopBar(
            section: _section,
            strings: strings,
            controller: controller,
            onSelect: (section) => setState(() => _section = section),
            onNotice: _showNotice,
          ),
          Divider(height: 1, color: scheme.outlineVariant),
          Expanded(
            child: controller.loading
                ? const Center(
                    child: SizedBox(
                      width: 20,
                      height: 20,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    ),
                  )
                : _buildSection(strings),
          ),
        ],
      ),
    );
  }
}

class _TopBar extends StatelessWidget {
  const _TopBar({
    required this.section,
    required this.strings,
    required this.controller,
    required this.onSelect,
    required this.onNotice,
  });

  final _Section section;
  final Strings strings;
  final EdgeSteerController controller;
  final ValueChanged<_Section> onSelect;
  final ValueChanged<String> onNotice;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final document = controller.document;
    final canSave = document != null &&
        document.isValid &&
        document.isDirty &&
        !controller.busy;

    return Container(
      height: 48,
      color: scheme.surfaceContainerLow,
      padding: const EdgeInsets.only(left: 14, right: 12),
      child: Row(
        children: [
          // The native title bar is transparent and its buttons are hidden, so
          // the lights are drawn here beside the logo.
          const WindowControls(),
          const SizedBox(width: 16),
          Image.asset(
            'assets/edgesteer-logo.png',
            width: 22,
            height: 22,
            // The 2x and 3x variants are exact-size, so this only matters if a
            // display density without a variant asks for a scaled copy.
            filterQuality: FilterQuality.medium,
          ),
          const SizedBox(width: 16),
          for (final entry in <(_Section, String)>[
            (_Section.status, strings.navStatus),
            (_Section.resolvers, strings.navResolvers),
            (_Section.ruleSets, strings.navRuleSets),
            (_Section.preferred, strings.navPreferred),
            (_Section.json, strings.navJson),
          ]) ...[
            _NavButton(
              label: entry.$2,
              selected: section == entry.$1,
              onTap: () => onSelect(entry.$1),
            ),
            const SizedBox(width: 1),
          ],
          const Spacer(),
          if (document != null)
            _SaveState(
              document: document,
              strings: strings,
              scheme: scheme,
            ),
          const SizedBox(width: 10),
          SizedBox(
            height: 28,
            child: FilledButton(
              onPressed: canSave
                  ? () async {
                      final error = await controller.saveDocument();
                      onNotice(error ?? strings.saved);
                    }
                  : null,
              style: FilledButton.styleFrom(
                padding: const EdgeInsets.symmetric(horizontal: 12),
                textStyle: const TextStyle(
                  fontSize: 12.5,
                  fontWeight: FontWeight.w500,
                ),
              ),
              child: Text(strings.save),
            ),
          ),
        ],
      ),
    );
  }
}

/// A single dot plus one word: enough to know whether saving is pending, and
/// nothing more.
class _SaveState extends StatelessWidget {
  const _SaveState({
    required this.document,
    required this.strings,
    required this.scheme,
  });

  final dynamic document;
  final Strings strings;
  final ColorScheme scheme;

  @override
  Widget build(BuildContext context) {
    final invalid = document.validationError != null;
    final dirty = document.isDirty;
    final label = invalid
        ? strings.invalid
        : dirty
            ? strings.unsaved
            : strings.saved;
    final color = invalid
        ? scheme.error
        : dirty
            ? scheme.primary
            : scheme.onSurfaceVariant;

    return Tooltip(
      message: invalid ? document.validationError as String : label,
      waitDuration: const Duration(milliseconds: 400),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: 6,
            height: 6,
            decoration: BoxDecoration(shape: BoxShape.circle, color: color),
          ),
          const SizedBox(width: 5),
          Text(label, style: TextStyle(fontSize: 11.5, color: color)),
        ],
      ),
    );
  }
}

class _NavButton extends StatelessWidget {
  const _NavButton({
    required this.label,
    required this.selected,
    required this.onTap,
  });

  final String label;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return TextButton(
      onPressed: onTap,
      style: TextButton.styleFrom(
        foregroundColor: selected ? scheme.primary : scheme.onSurfaceVariant,
        backgroundColor:
            selected ? scheme.primary.withValues(alpha: 0.12) : null,
        padding: const EdgeInsets.symmetric(horizontal: 11),
        minimumSize: const Size(0, 30),
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
      ),
      child: Text(
        label,
        style: TextStyle(
          fontSize: 12.5,
          fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
        ),
      ),
    );
  }
}
