/// EdgeSteer configuration interface.
///
/// The window is a separate process from the menu-bar agent: closing it ends
/// only this process, and the agent keeps serving DNS. The interface never
/// touches the DNS engine, the system DNS settings, or the configuration file
/// directly; every operation goes through the agent's authenticated control
/// protocol.
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
        final strings = _controller.strings;
        return MaterialApp(
          title: 'EdgeSteer',
          debugShowCheckedModeBanner: false,
          theme: buildTheme(dark: _controller.preferences.isDark),
          home: EdgeSteerScope(
            controller: _controller,
            child: EdgeSteerShell(strings: strings),
          ),
        );
      },
    );
  }
}

enum _Page { overview, resolvers, ruleSets, cloudflare, json, system }

class EdgeSteerShell extends StatefulWidget {
  const EdgeSteerShell({super.key, required this.strings});

  final Strings strings;

  @override
  State<EdgeSteerShell> createState() => _EdgeSteerShellState();
}

class _EdgeSteerShellState extends State<EdgeSteerShell> {
  _Page _page = _initialPage();
  bool _noticeShown = false;

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

  /// `EDGESTEER_UI_PAGE=<name>` opens a specific page. It exists so the packaged
  /// interface can be screenshotted page by page during verification; normal
  /// launches always start on the overview.
  static _Page _initialPage() {
    final requested = Platform.environment['EDGESTEER_UI_PAGE'];
    if (requested == null) {
      return _Page.overview;
    }
    return _Page.values.firstWhere(
      (page) => page.name == requested,
      orElse: () => _Page.overview,
    );
  }

  void _showNotice(String message) {
    if (!mounted || message.isEmpty) {
      return;
    }
    ScaffoldMessenger.of(context)
      ..clearSnackBars()
      ..showSnackBar(
        SnackBar(
          content: Text(message),
          duration: const Duration(seconds: 5),
        ),
      );
  }


  /// Builds only the selected page. An IndexedStack would lay every page out at
  /// the largest intrinsic size, which collapses the scrollable pages inside a
  /// window that is smaller than their content.
  Widget _buildPage(Strings strings) {
    final notice = _showNotice;
    return switch (_page) {
      _Page.overview => OverviewPage(
          strings: strings,
          onNotice: notice,
          onOpenResolvers: () => setState(() => _page = _Page.resolvers),
        ),
      _Page.resolvers => ResolverPage(strings: strings, onNotice: notice),
      _Page.ruleSets => RulesPage(strings: strings, onNotice: notice),
      _Page.cloudflare => CloudflarePage(strings: strings, onNotice: notice),
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
                ? const Center(child: CircularProgressIndicator())
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
      height: 58,
      color: scheme.surfaceContainerLow,
      padding: const EdgeInsets.only(left: 78, right: 18),
      child: Row(
        children: [
          Image.asset(
            'assets/edgesteer-logo.png',
            width: 26,
            height: 26,
            filterQuality: FilterQuality.high,
          ),
          const SizedBox(width: 10),
          Text(
            'EdgeSteer',
            style: TextStyle(
              fontSize: 17,
              fontWeight: FontWeight.w600,
              color: scheme.onSurface,
            ),
          ),
          const SizedBox(width: 18),
          // Navigation scrolls horizontally on narrow windows instead of
          // squeezing the save action off the edge.
          Expanded(
            child: SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: Row(
                children: [
                  for (final entry in <(_Page, String)>[
                    (_Page.overview, strings.navOverview),
                    (_Page.resolvers, strings.navResolvers),
                    (_Page.ruleSets, strings.navRuleSets),
                    (_Page.cloudflare, strings.navCloudflare),
                    (_Page.json, strings.navJson),
                    (_Page.system, strings.navSystem),
                  ]) ...[
                    _NavButton(
                      label: entry.$2,
                      selected: page == entry.$1,
                      onTap: () => onSelect(entry.$1),
                    ),
                    const SizedBox(width: 2),
                  ],
                ],
              ),
            ),
          ),
          const SizedBox(width: 12),
          if (document != null)
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 220),
              child: Tooltip(
                message: document.validationSummary,
                child: Text(
                  document.validationSummary,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  textAlign: TextAlign.right,
                  style: TextStyle(
                    fontSize: 12,
                    color: document.validationError != null
                        ? scheme.error
                        : scheme.onSurfaceVariant,
                  ),
                ),
              ),
            ),
          const SizedBox(width: 12),
          FilledButton.icon(
            onPressed: canSave
                ? () async {
                    final error = await controller.saveDocument();
                    onNotice(
                      error ?? controller.strings.configurationSaved,
                    );
                  }
                : null,
            icon: const Icon(Icons.save_outlined, size: 15),
            label: Text(strings.saveConfiguration),
          ),
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
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 16),
      ),
      child: Text(
        label,
        style: TextStyle(
          fontSize: 13,
          fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
        ),
      ),
    );
  }
}
