/// Theme for the EdgeSteer interface: the same orange accent and dark/light
/// panels the native menu-bar application used, expressed as a Material 3
/// `ColorScheme`.
library;

import 'package:flutter/material.dart';

const Color edgeOrange = Color(0xFFF48120);
const Color edgeOrangeHover = Color(0xFFE06F13);
const Color edgeDanger = Color(0xFFB84A3A);

ThemeData buildTheme({required bool dark}) {
  final scheme = dark ? _darkScheme() : _lightScheme();
  return ThemeData(
    useMaterial3: true,
    brightness: dark ? Brightness.dark : Brightness.light,
    colorScheme: scheme,
    scaffoldBackgroundColor: scheme.surface,
    fontFamily: '.AppleSystemUIFont',
    visualDensity: VisualDensity.compact,
    dividerTheme: DividerThemeData(
      color: scheme.outlineVariant,
      thickness: 1,
      space: 1,
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        backgroundColor: edgeOrange,
        foregroundColor: Colors.white,
        disabledBackgroundColor: scheme.surfaceContainerHighest,
        disabledForegroundColor: scheme.onSurfaceVariant,
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 16),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
        textStyle: const TextStyle(fontSize: 13, fontWeight: FontWeight.w500),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        foregroundColor: scheme.onSurface,
        side: BorderSide(color: scheme.outlineVariant),
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 16),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
        textStyle: const TextStyle(fontSize: 13),
      ),
    ),
    textButtonTheme: TextButtonThemeData(
      style: TextButton.styleFrom(
        foregroundColor: scheme.onSurfaceVariant,
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 16),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
        textStyle: const TextStyle(fontSize: 13),
      ),
    ),
    inputDecorationTheme: InputDecorationTheme(
      isDense: true,
      filled: true,
      fillColor: scheme.surfaceContainerHighest.withValues(alpha: 0.45),
      contentPadding: const EdgeInsets.symmetric(horizontal: 10, vertical: 12),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(6),
        borderSide: BorderSide(color: scheme.outlineVariant),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(6),
        borderSide: BorderSide(color: scheme.outlineVariant),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(6),
        borderSide: const BorderSide(color: edgeOrange),
      ),
      hintStyle: TextStyle(fontSize: 12, color: scheme.onSurfaceVariant),
    ),
    dropdownMenuTheme: DropdownMenuThemeData(
      textStyle: TextStyle(fontSize: 13, color: scheme.onSurface),
    ),
    snackBarTheme: SnackBarThemeData(
      behavior: SnackBarBehavior.floating,
      backgroundColor: scheme.surfaceContainerHigh,
      contentTextStyle: TextStyle(color: scheme.onSurface, fontSize: 13),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(7),
        side: const BorderSide(color: edgeOrange),
      ),
    ),
    scrollbarTheme: ScrollbarThemeData(
      thumbColor: WidgetStatePropertyAll(
        scheme.onSurfaceVariant.withValues(alpha: 0.35),
      ),
      thickness: const WidgetStatePropertyAll(8),
    ),
  );
}

ColorScheme _darkScheme() => const ColorScheme.dark(
      primary: edgeOrange,
      onPrimary: Colors.white,
      secondary: edgeOrange,
      onSecondary: Colors.white,
      error: edgeDanger,
      onError: Colors.white,
      surface: Color(0xFF15171A),
      onSurface: Color(0xFFEDEFF2),
      surfaceContainerLowest: Color(0xFF101214),
      surfaceContainerLow: Color(0xFF1A1D21),
      surfaceContainer: Color(0xFF1E2227),
      surfaceContainerHigh: Color(0xFF23272D),
      surfaceContainerHighest: Color(0xFF2A2F36),
      onSurfaceVariant: Color(0xFFA7AEB8),
      outline: Color(0xFF3C434C),
      outlineVariant: Color(0xFF2E343B),
    );

ColorScheme _lightScheme() => const ColorScheme.light(
      primary: edgeOrange,
      onPrimary: Colors.white,
      secondary: edgeOrange,
      onSecondary: Colors.white,
      error: edgeDanger,
      onError: Colors.white,
      surface: Color(0xFFF7F8FA),
      onSurface: Color(0xFF1A1D21),
      surfaceContainerLowest: Color(0xFFFFFFFF),
      surfaceContainerLow: Color(0xFFFFFFFF),
      surfaceContainer: Color(0xFFF1F3F6),
      surfaceContainerHigh: Color(0xFFEDEFF3),
      surfaceContainerHighest: Color(0xFFE6E9EE),
      onSurfaceVariant: Color(0xFF5B6472),
      outline: Color(0xFFC5CBD4),
      outlineVariant: Color(0xFFDDE1E7),
    );
