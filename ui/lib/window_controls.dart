/// macOS window controls drawn inside the app's own top bar.
///
/// The native title bar is transparent and its buttons are hidden, so the
/// traffic lights sit beside the logo in the content instead of in a separate
/// strip above it. These widgets keep the platform behaviour: the colours and
/// glyphs appear on hover, and each button drives the real window through a
/// small method channel.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

const MethodChannel _windowChannel = MethodChannel('io.edgesteer/window');

/// The three lights as a group.
class WindowControls extends StatelessWidget {
  const WindowControls({super.key});

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: const [
        _Light(
          method: 'close',
          color: Color(0xFFFF5F57),
          borderColor: Color(0xFFE0443E),
          glyph: _Glyph.close,
          tooltip: '关闭',
        ),
        SizedBox(width: 8),
        _Light(
          method: 'minimize',
          color: Color(0xFFFEBC2E),
          borderColor: Color(0xFFDEA123),
          glyph: _Glyph.minimize,
          tooltip: '最小化',
        ),
        SizedBox(width: 8),
        _Light(
          method: 'zoom',
          color: Color(0xFF28C840),
          borderColor: Color(0xFF1DAD2B),
          glyph: _Glyph.zoom,
          tooltip: '缩放',
        ),
      ],
    );
  }
}

enum _Glyph { close, minimize, zoom }

class _Light extends StatefulWidget {
  const _Light({
    required this.method,
    required this.color,
    required this.borderColor,
    required this.glyph,
    required this.tooltip,
  });

  final String method;
  final Color color;
  final Color borderColor;
  final _Glyph glyph;
  final String tooltip;

  @override
  State<_Light> createState() => _LightState();
}

class _LightState extends State<_Light> {
  bool _hovered = false;

  Future<void> _activate() async {
    try {
      await _windowChannel.invokeMethod<void>(widget.method);
    } on PlatformException {
      // A window that cannot act on the request is not worth an error surface:
      // the lights stay usable and the platform decides.
    } on MissingPluginException {
      // Running outside the macOS host (tests, other platforms).
    }
  }

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: widget.tooltip,
      waitDuration: const Duration(milliseconds: 600),
      child: MouseRegion(
        cursor: SystemMouseCursors.basic,
        onEnter: (_) => setState(() => _hovered = true),
        onExit: (_) => setState(() => _hovered = false),
        child: GestureDetector(
          onTap: _activate,
          child: Container(
            width: 12,
            height: 12,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: _hovered ? widget.color : widget.color.withValues(alpha: 0.9),
              border: Border.all(
                color: widget.borderColor.withValues(alpha: 0.8),
                width: 0.5,
              ),
            ),
            child: _hovered
                ? Center(child: _glyphIcon())
                : const SizedBox.shrink(),
          ),
        ),
      ),
    );
  }

  Widget _glyphIcon() {
    // macOS draws these glyphs at roughly half the dot size. The stock icons
    // carry padding, so constrain them explicitly instead of trusting `size`.
    final color = const Color(0xFF3B0000).withValues(alpha: 0.62);
    final icon = switch (widget.glyph) {
      _Glyph.close => Icons.close,
      _Glyph.minimize => Icons.remove,
      _Glyph.zoom => Icons.open_in_full,
    };
    return SizedBox(
      width: 8,
      height: 8,
      child: FittedBox(
        fit: BoxFit.contain,
        child: Icon(icon, size: 12, color: color),
      ),
    );
  }
}
