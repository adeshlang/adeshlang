import 'package:flutter/material.dart';

class AdeshSyntax {
  static const Color keywordColor = Color(0xFFFF7B72);
  static const Color typeColor = Color(0xFF79C0FF);
  static const Color literalColor = Color(0xFF7EE787);
  static const Color stringColor = Color(0xFFA5D6FF);
  static const Color numberColor = Color(0xFF79C0FF);
  static const Color commentColor = Color(0xFF8B949E);
  static const Color decoratorColor = Color(0xFFD2A8FF);
  static const Color operatorColor = Color(0xFFFFA657);
  static const Color functionColor = Color(0xFFD2A8FF);
  static const Color defaultTextColor = Color(0xFFC9D1D9);

  static const Set<String> keywords = {
    'fn', 'let', 'mut', 'if', 'else', 'for', 'while', 'loop', 'match',
    'return', 'struct', 'enum', 'class', 'trait', 'impl', 'import', 'export',
    'use', 'pub', 'async', 'await', 'spawn', 'yield', 'const', 'static',
    'share', 'unsafe', 'type', 'in', 'break', 'continue', 'as'
  };

  static const Set<String> typesAndBuiltins = {
    'i8', 'i16', 'i32', 'i64', 'i128', 'u8', 'u16', 'u32', 'u64', 'u128',
    'f32', 'f64', 'bool', 'char', 'str', 'String', 'Option', 'Result', 'Vec',
    'print', 'println', 'eprint', 'eprintln', 'len', 'push', 'pop', 'read_line',
    'assert', 'clock', 'format'
  };

  static const Set<String> literals = {
    'true', 'false', 'null', 'Ok', 'Err', 'Some', 'None'
  };

  static TextSpan highlight(String code, TextStyle baseStyle) {
    if (code.isEmpty) {
      return TextSpan(style: baseStyle, text: '');
    }

    final List<TextSpan> spans = [];
    int index = 0;
    final int len = code.length;

    while (index < len) {
      if (index + 1 < len && code[index] == '/' && code[index + 1] == '/') {
        int end = code.indexOf('\n', index);
        if (end == -1) end = len;
        spans.add(TextSpan(
          text: code.substring(index, end),
          style: baseStyle.copyWith(color: commentColor, fontStyle: FontStyle.italic),
        ));
        index = end;
        continue;
      }

      if (index + 1 < len && code[index] == '/' && code[index + 1] == '*') {
        int end = code.indexOf('*/', index + 2);
        if (end == -1) {
          end = len;
        } else {
          end += 2;
        }
        spans.add(TextSpan(
          text: code.substring(index, end),
          style: baseStyle.copyWith(color: commentColor, fontStyle: FontStyle.italic),
        ));
        index = end;
        continue;
      }

      if (code[index] == '"' || code[index] == "'") {
        final quote = code[index];
        int end = index + 1;
        while (end < len) {
          if (code[end] == quote && code[end - 1] != '\\') {
            end++;
            break;
          }
          if (code[end] == '\n') break;
          end++;
        }
        spans.add(TextSpan(
          text: code.substring(index, end),
          style: baseStyle.copyWith(color: stringColor),
        ));
        index = end;
        continue;
      }

      if (code[index] == '@') {
        int end = index + 1;
        while (end < len && (code[end].contains(RegExp(r'[a-zA-Z0-9_]')))) {
          end++;
        }
        spans.add(TextSpan(
          text: code.substring(index, end),
          style: baseStyle.copyWith(color: decoratorColor, fontWeight: FontWeight.bold),
        ));
        index = end;
        continue;
      }

      if (code[index].contains(RegExp(r'[a-zA-Z_]'))) {
        int end = index + 1;
        while (end < len && code[end].contains(RegExp(r'[a-zA-Z0-9_]'))) {
          end++;
        }
        final word = code.substring(index, end);
        Color color = defaultTextColor;
        FontWeight weight = FontWeight.normal;

        if (keywords.contains(word)) {
          color = keywordColor;
          weight = FontWeight.bold;
        } else if (typesAndBuiltins.contains(word)) {
          color = typeColor;
        } else if (literals.contains(word)) {
          color = literalColor;
          weight = FontWeight.bold;
        } else if (end < len && code[end] == '(') {
          color = functionColor;
        }

        spans.add(TextSpan(
          text: word,
          style: baseStyle.copyWith(color: color, fontWeight: weight),
        ));
        index = end;
        continue;
      }

      if (code[index].contains(RegExp(r'[0-9]'))) {
        int end = index + 1;
        while (end < len && code[end].contains(RegExp(r'[0-9\.xX0-9a-fA-F]'))) {
          end++;
        }
        spans.add(TextSpan(
          text: code.substring(index, end),
          style: baseStyle.copyWith(color: numberColor),
        ));
        index = end;
        continue;
      }

      if ('+-*/=<>!&|:;,.{}()[]'.contains(code[index])) {
        spans.add(TextSpan(
          text: code[index],
          style: baseStyle.copyWith(color: operatorColor),
        ));
        index++;
        continue;
      }

      spans.add(TextSpan(
        text: code[index],
        style: baseStyle.copyWith(color: defaultTextColor),
      ));
      index++;
    }

    return TextSpan(style: baseStyle, children: spans);
  }
}
