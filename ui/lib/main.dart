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

enum _Page { status, resolvers, ruleSets, preferred, json, system }

class EdgeSteerShell extends StatefulWidget {
  const EdgeSteerShell({super.key, required this.strings});

  final Strings strings;

  @override
  State<EdgeSteerShell> createState() => _EdgeSteerShellState();
}

class _EdgeSteerShellState extends State<EdgeSteerShell> {
  _Page _page = _initialPage();
  bool _noticeShown = false;

  /// `EDGESTEER_UI_PAGE=<name>` opens a specific page. Used by verification
  /// scripts and by agents driving the packaged interface; normal launches open
  /// the status page.
  static _Page _initialPage() {
    final requested = Platform.environment['EDGESTEER_UI_PAGE'];
    if (requested == null) {
      return _Page.status;
    }
    return _Page.values.firstWhere(
      (page) => page.name == requested,
      orElse: () => _Page.status,
    );
  }

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

  Widget _buildPage(Strings strings) {
    final notice = _showNotice;
    return switch (_page) {
      _Page.status => OverviewPage(
          strings: strings,
          onNotice: notice,
          onOpenResolvers: () => setState(() => _page = _Page.resolvers),
        ),
      _Page.resolvers => ResolverPage(strings: strings, onNotice: notice),
      _Page.ruleSets => RulesPage(strings: strings, onNotice: notice),
      _Page.preferred => CloudflarePage(strings: strings, onNotice: notice),
      _Page.json => JsonPage(strings: strings, onNotice: notice),
      _Page.system => SystemPage(
          strings: strings,
          onNotice: notice,
          onQuit: () => exit(0),
        ),
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
            page: _page,
            strings: strings,
            controller: controller,
            onSelect: (page) => setState(() => _page = page),
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
                : _buildPage(strings),
          ),
        ],
      ),
    );
  }
}

class _TopBar extends StatelessWidget {
  const _TopBar({
    required this.page,
    required this.strings,
    required this.controller,
    required this.onSelect,
    required this.onNotice,
  });

  final _Page page;
  final Strings strings;
  final EdgeSteerController controller;
  final ValueChanged<_Page> onSelect;
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
      padding: const EdgeInsets.only(left: 76, right: 12),
      child: Row(
        children: [
          Image.asset(
            'assets/edgesteer-logo.png',
            width: 20,
            height: 20,
            filterQuality: FilterQuality.high,
          ),
          const SizedBox(width: 16),
          for (final entry in <(_Page, String)>[
            (_Page.status, strings.navStatus),
            (_Page.resolvers, strings.navResolvers),
            (_Page.ruleSets, strings.navRuleSets),
            (_Page.preferred, strings.navPreferred),
            (_Page.json, strings.navJson),
            (_Page.system, strings.navSystem),
          ]) ...[
            _NavButton(
              label: entry.$2,
              selected: page == entry.$1,
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
