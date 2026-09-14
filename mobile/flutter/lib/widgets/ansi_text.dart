import 'package:flutter/material.dart';

/// Parses a string containing ANSI SGR escape sequences and returns
/// a list of [TextSpan]s with the appropriate [TextStyle]s applied.
///
/// Supported codes:
///   0       — reset
///   1       — bold
///   2       — dim (faint)
///   3       — italic
///   4       — underline
///   22      — normal intensity (cancel bold/dim)
///   23      — not italic
///   24      — not underline
///   30–37   — standard foreground colors
///   38;5;n  — 256-color foreground
///   38;2;r;g;b — true-color foreground
///   39      — default foreground
///   40–47   — standard background colors
///   48;5;n  — 256-color background
///   48;2;r;g;b — true-color background
///   49      — default background
///   90–97   — bright foreground colors
///   100–107 — bright background colors
class AnsiTextSpanParser {
  // Standard ANSI 16 colors (normal + bright)
  static const List<Color> _ansiColors = [
    Color(0xFF000000), // 0: black
    Color(0xFFCC0000), // 1: red
    Color(0xFF00CC00), // 2: green
    Color(0xFFCC7A00), // 3: yellow
    Color(0xFF0000CC), // 4: blue
    Color(0xFFCC00CC), // 5: magenta
    Color(0xFF00CCCC), // 6: cyan
    Color(0xFFCCCCCC), // 7: white
    Color(0xFF666666), // 8: bright black (dark gray)
    Color(0xFFFF4444), // 9: bright red
    Color(0xFF44FF44), // 10: bright green
    Color(0xFFFFFF44), // 11: bright yellow
    Color(0xFF4444FF), // 12: bright blue
    Color(0xFFFF44FF), // 13: bright magenta
    Color(0xFF44FFFF), // 14: bright cyan
    Color(0xFFFFFFFF), // 15: bright white
  ];

  /// Builds a 256-color palette (xterm-256) on demand.
  static Color _color256(int n) {
    if (n < 16) return _ansiColors[n];
    if (n >= 232) {
      // Grayscale ramp 232-255
      final v = (n - 232) * 10 + 8;
      return Color.fromARGB(0xFF, v, v, v);
    }
    // 6×6×6 color cube (16-231)
    final i = n - 16;
    final r = ((i ~/ 36) % 6) * 51;
    final g = ((i ~/ 6) % 6) * 51;
    final b = (i % 6) * 51;
    return Color.fromARGB(0xFF, r, g, b);
  }

  /// Parse the [input] string and return colored [TextSpan]s.
  static List<TextSpan> parse(String input, {TextStyle? baseStyle}) {
    final spans = <TextSpan>[];
    // Regex that matches an ESC [ ... m sequence (SGR)
    final ansiRe = RegExp(r'\x1b\[([0-9;]*)m');

    // Current state
    Color? fg;
    Color? bg;
    bool bold = false;
    bool italic = false;
    bool underline = false;

    TextStyle currentStyle() {
      return (baseStyle ?? const TextStyle()).copyWith(
        color: fg,
        backgroundColor: bg,
        fontWeight: bold ? FontWeight.bold : FontWeight.normal,
        fontStyle: italic ? FontStyle.italic : FontStyle.normal,
        decoration: underline ? TextDecoration.underline : TextDecoration.none,
      );
    }

    int cursor = 0;
    for (final match in ansiRe.allMatches(input)) {
      // Emit text before this escape
      if (match.start > cursor) {
        final text = input.substring(cursor, match.start);
        if (text.isNotEmpty) {
          spans.add(TextSpan(text: text, style: currentStyle()));
        }
      }
      cursor = match.end;

      // Parse the numeric parameters
      final raw = match.group(1) ?? '';
      final params = raw.isEmpty ? [0] : raw.split(';').map(int.tryParse).toList();

      int i = 0;
      while (i < params.length) {
        final code = params[i] ?? 0;
        switch (code) {
          case 0: // Reset
            fg = null; bg = null; bold = false; italic = false; underline = false;
          case 1: bold = true;
          case 2: bold = false; // dim — treat as not bold
          case 3: italic = true;
          case 4: underline = true;
          case 22: bold = false;
          case 23: italic = false;
          case 24: underline = false;
          case 30: fg = _ansiColors[0];
          case 31: fg = _ansiColors[1];
          case 32: fg = _ansiColors[2];
          case 33: fg = _ansiColors[3];
          case 34: fg = _ansiColors[4];
          case 35: fg = _ansiColors[5];
          case 36: fg = _ansiColors[6];
          case 37: fg = _ansiColors[7];
          case 38: // extended fg
            if (i + 1 < params.length && params[i + 1] == 5 && i + 2 < params.length) {
              fg = _color256(params[i + 2] ?? 0);
              i += 2;
            } else if (i + 1 < params.length && params[i + 1] == 2 && i + 4 < params.length) {
              fg = Color.fromARGB(0xFF, params[i + 2] ?? 0, params[i + 3] ?? 0, params[i + 4] ?? 0);
              i += 4;
            }
          case 39: fg = null;
          case 40: bg = _ansiColors[0];
          case 41: bg = _ansiColors[1];
          case 42: bg = _ansiColors[2];
          case 43: bg = _ansiColors[3];
          case 44: bg = _ansiColors[4];
          case 45: bg = _ansiColors[5];
          case 46: bg = _ansiColors[6];
          case 47: bg = _ansiColors[7];
          case 48: // extended bg
            if (i + 1 < params.length && params[i + 1] == 5 && i + 2 < params.length) {
              bg = _color256(params[i + 2] ?? 0);
              i += 2;
            } else if (i + 1 < params.length && params[i + 1] == 2 && i + 4 < params.length) {
              bg = Color.fromARGB(0xFF, params[i + 2] ?? 0, params[i + 3] ?? 0, params[i + 4] ?? 0);
              i += 4;
            }
          case 49: bg = null;
          case 90: fg = _ansiColors[8];
          case 91: fg = _ansiColors[9];
          case 92: fg = _ansiColors[10];
          case 93: fg = _ansiColors[11];
          case 94: fg = _ansiColors[12];
          case 95: fg = _ansiColors[13];
          case 96: fg = _ansiColors[14];
          case 97: fg = _ansiColors[15];
          case 100: bg = _ansiColors[8];
          case 101: bg = _ansiColors[9];
          case 102: bg = _ansiColors[10];
          case 103: bg = _ansiColors[11];
          case 104: bg = _ansiColors[12];
          case 105: bg = _ansiColors[13];
          case 106: bg = _ansiColors[14];
          case 107: bg = _ansiColors[15];
          default: break;
        }
        i++;
      }
    }

    // Emit remaining text after last escape
    if (cursor < input.length) {
      final tail = input.substring(cursor);
      if (tail.isNotEmpty) {
        spans.add(TextSpan(text: tail, style: currentStyle()));
      }
    }

    // If nothing was parsed (no ANSI codes), return plain span
    if (spans.isEmpty) {
      spans.add(TextSpan(text: input, style: baseStyle));
    }

    return spans;
  }

  /// Returns true if the string contains any ANSI escape sequences.
  static bool hasAnsi(String text) => text.contains('\x1b[');

  /// Strips all ANSI escape sequences from a string (for clean copy/share).
  static String strip(String text) =>
      text.replaceAll(RegExp(r'\x1b\[[0-9;]*[a-zA-Z]'), '');
}

/// A widget that renders ANSI-colored terminal output.
/// Falls back to a plain [SelectableText] when there are no ANSI codes.
class AnsiText extends StatelessWidget {
  final String text;
  final TextStyle? style;

  const AnsiText(this.text, {super.key, this.style});

  @override
  Widget build(BuildContext context) {
    if (!AnsiTextSpanParser.hasAnsi(text)) {
      return SelectableText(text, style: style);
    }
    final spans = AnsiTextSpanParser.parse(text, baseStyle: style);
    return SelectableText.rich(TextSpan(children: spans));
  }
}
