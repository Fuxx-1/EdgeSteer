/// EdgeSteer configuration interface.
///
/// This process owns the window only. Closing it ends this process and nothing
/// else: the menu-bar agent keeps the Rust DNS engine running. Every operation
/// goes through the agent's authenticated control protocol, so the window never
/// touches the resolver, system DNS, or the configuration file directly.
library;

import 'dart:io';

import 'package:flutter/material.dart';

import 'app_state.dart';
import 'pages/cloudflare_page.dart';
import 'pages/json_page.dart';
import 'pages/overview_page.dart';
import 'pages/resolver_page.dart';
import 'pages/rules_page.dart';
import 'pages/system_page.dart';
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
        return MaterialApp(
          title: 'EdgeSteer',
          debugShowCheckedModeBanner: false,
          theme: buildTheme(dark: _controller.preferences.isDark),
          home: EdgeSteerScope(
            controller: _controller,
            child: EdgeSteerShell(strings: _controller.strings),
          ),
        );
      },
    );
  }
}

/// Top-level sections. Configuration is one section with four views inside it,
/// because resolvers, rule sets, preferred plugins, and the raw JSON all edit
/// the same document.
enum _Section { status, config, system }

/// Views inside the configuration section.
enum _ConfigView { resolvers, ruleSets, preferred, json }

class EdgeSteerShell extends StatefulWidget {
  const EdgeSteerShell({super.key, required this.strings});

  final Strings strings;

  @override
  State<EdgeSteerShell> createState() => _EdgeSteerShellState();
}

class _EdgeSteerShellState extends State<EdgeSteerShell> {
  late _Section _section;
  late _ConfigView _configView;
  bool _noticeShown = false;

  @override
  void initState() {
    super.initState();
    final requested = _requestedPage();
    _section = _sectionFor(requested);
    _configView = _configViewFor(requested);
  }

  /// `EDGESTEER_UI_PAGE=<name>` opens a specific page. Used by verification
  /// scripts and by agents driving the packaged interface; normal launches open
  /// the status section. The accepted names are unchanged from when every view
  /// had its own top-level tab, so existing scripts keep working.
  static String? _requestedPage() =>
      Platform.environment['EDGESTEER_UI_PAGE'];

  static _Section _sectionFor(String? requested) => switch (requested) {
        'system' => _Section.system,
        'config' || 'resolvers' || 'ruleSets' || 'preferred' || 'json' =>
          _Section.config,
        _ => _Section.status,
      };

  static _ConfigView _configViewFor(String? requested) => switch (requested) {
        'ruleSets' => _ConfigView.ruleSets,
        'preferred' => _ConfigView.preferred,
        'json' => _ConfigView.json,
        _ => _ConfigView.resolvers,
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
          onOpenResolvers: () => setState(() {
            _section = _Section.config;
            _configView = _ConfigView.resolvers;
          }),
        ),
      _Section.config => _buildConfig(strings, notice),
      _Section.system => SystemPage(
          strings: strings,
          onNotice: notice,
          onQuit: () => exit(0),
        ),
    };
  }

  /// One document, four views. Switching a view must not lose unsaved edits, so
  /// the views are built on demand from the shared document rather than kept
  /// alive with their own copies.
  Widget _buildConfig(Strings strings, ValueChanged<String> notice) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _ConfigTabs(
          view: _configView,
          strings: strings,
          onSelect: (view) => setState(() => _configView = view),
        ),
        Expanded(
          child: switch (_configView) {
            _ConfigView.resolvers =>
              ResolverPage(strings: strings, onNotice: notice),
            _ConfigView.ruleSets =>
              RulesPage(strings: strings, onNotice: notice),
            _ConfigView.preferred =>
              CloudflarePage(strings: strings, onNotice: notice),
            _ConfigView.json => JsonPage(strings: strings, onNotice: notice),
          },
        ),
      ],
    );
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
            (_Section.config, strings.navConfig),
            (_Section.system, strings.navSystem),
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

/// Segmented control for the views inside the configuration section.
///
/// These are not navigation: all four edit the same document, so they read as a
/// mode switch under the section title rather than as more tabs competing with
/// the top bar.
class _ConfigTabs extends StatelessWidget {
  const _ConfigTabs({
    required this.view,
    required this.strings,
    required this.onSelect,
  });

  final _ConfigView view;
  final Strings strings;
  final ValueChanged<_ConfigView> onSelect;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final entries = <(_ConfigView, String)>[
      (_ConfigView.resolvers, strings.sectionResolvers),
      (_ConfigView.ruleSets, strings.sectionRuleSets),
      (_ConfigView.preferred, strings.sectionPreferred),
      (_ConfigView.json, strings.sectionJson),
    ];

    return Container(
      height: 34,
      color: scheme.surfaceContainerLow,
      padding: const EdgeInsets.symmetric(horizontal: 14),
      child: Row(
        children: [
          for (final entry in entries) ...[
            _Segment(
              label: entry.$2,
              selected: view == entry.$1,
              onTap: () => onSelect(entry.$1),
            ),
            const SizedBox(width: 2),
          ],
        ],
      ),
    );
  }
}

class _Segment extends StatelessWidget {
  const _Segment({
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
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(5),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
        decoration: BoxDecoration(
          color: selected ? scheme.primary.withValues(alpha: 0.14) : null,
          borderRadius: BorderRadius.circular(5),
        ),
        child: Text(
          label,
          style: TextStyle(
            fontSize: 12,
            fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
            color: selected ? scheme.primary : scheme.onSurfaceVariant,
          ),
        ),
      ),
    );
  }
}
