import 'package:flutter/material.dart';

import 'adesh_syntax.dart';

class UndoHistoryItem {
  final String text;
  final TextSelection selection;

  UndoHistoryItem(this.text, this.selection);
}

class AdeshCodeController extends TextEditingController {
  bool enableSyntaxHighlighting;
  bool autoCloseBrackets;
  int tabSize;

  final List<UndoHistoryItem> _undoStack = [];
  final List<UndoHistoryItem> _redoStack = [];
  bool _isUndoingOrRedoing = false;
  bool _internalUpdate = false;

  AdeshCodeController({
    String? text,
    this.enableSyntaxHighlighting = true,
    this.autoCloseBrackets = true,
    this.tabSize = 4,
  }) : super(text: text) {
    if (text != null && text.isNotEmpty) {
      _undoStack.add(UndoHistoryItem(text, const TextSelection.collapsed(offset: 0)));
    }
  }

  bool get canUndo => _undoStack.length > 1;
  bool get canRedo => _redoStack.isNotEmpty;

  void pushUndoState() {
    if (_isUndoingOrRedoing || _internalUpdate) return;
    final item = UndoHistoryItem(text, selection);
    if (_undoStack.isEmpty || _undoStack.last.text != text) {
      _undoStack.add(item);
      if (_undoStack.length > 100) _undoStack.removeAt(0);
      _redoStack.clear();
    }
  }

  void undo() {
    if (!canUndo) return;
    _isUndoingOrRedoing = true;
    final current = _undoStack.removeLast();
    _redoStack.add(current);
    final previous = _undoStack.last;
    value = TextEditingValue(
      text: previous.text,
      selection: previous.selection,
      composing: TextRange.empty,
    );
    _isUndoingOrRedoing = false;
  }

  void redo() {
    if (!canRedo) return;
    _isUndoingOrRedoing = true;
    final item = _redoStack.removeLast();
    _undoStack.add(item);
    value = TextEditingValue(
      text: item.text,
      selection: item.selection,
      composing: TextRange.empty,
    );
    _isUndoingOrRedoing = false;
  }

  @override
  set value(TextEditingValue newValue) {
    if (_isUndoingOrRedoing || _internalUpdate) {
      super.value = newValue;
      return;
    }

    final oldText = text;
    final oldSel = selection;
    final newText = newValue.text;
    final newSel = newValue.selection;

    // 1. If only selection/cursor changed (no text alteration), do not push undo or auto-process
    if (oldText == newText) {
      super.value = newValue;
      return;
    }

    // 2. Handle auto-indent on newline insertion (Enter key)
    if (oldSel.isValid &&
        oldSel.isCollapsed &&
        newSel.isValid &&
        newSel.isCollapsed &&
        newText.length == oldText.length + 1 &&
        newSel.baseOffset == oldSel.baseOffset + 1 &&
        newText[oldSel.baseOffset] == '\n') {
      final insertPos = oldSel.baseOffset;
      final lineStart = insertPos == 0 ? 0 : oldText.lastIndexOf('\n', insertPos - 1) + 1;
      final currentLine = oldText.substring(lineStart, insertPos);
      final indentMatch = RegExp(r'^\s*').firstMatch(currentLine);
      final baseIndent = indentMatch != null ? indentMatch.group(0) ?? '' : '';

      final isPrevOpenBrace = currentLine.trimRight().endsWith('{') ||
          currentLine.trimRight().endsWith('(') ||
          currentLine.trimRight().endsWith('[');

      final isNextCloseBrace = insertPos < oldText.length &&
          (oldText[insertPos] == '}' || oldText[insertPos] == ')' || oldText[insertPos] == ']');

      if (isPrevOpenBrace && isNextCloseBrace && oldText[insertPos] == '}') {
        // Expanded block: {\n    |\n}
        final extraIndent = ' ' * tabSize;
        final expanded = '\n$baseIndent$extraIndent\n$baseIndent';
        final modifiedText = '${oldText.substring(0, insertPos)}$expanded${oldText.substring(insertPos)}';
        _internalUpdate = true;
        super.value = TextEditingValue(
          text: modifiedText,
          selection: TextSelection.collapsed(offset: insertPos + 1 + baseIndent.length + extraIndent.length),
          composing: TextRange.empty,
        );
        _internalUpdate = false;
        pushUndoState();
        return;
      } else if (isPrevOpenBrace) {
        final extraIndent = ' ' * tabSize;
        final fullIndent = '$baseIndent$extraIndent';
        final modifiedText = '${oldText.substring(0, insertPos)}\n$fullIndent${oldText.substring(insertPos)}';
        _internalUpdate = true;
        super.value = TextEditingValue(
          text: modifiedText,
          selection: TextSelection.collapsed(offset: insertPos + 1 + fullIndent.length),
          composing: TextRange.empty,
        );
        _internalUpdate = false;
        pushUndoState();
        return;
      } else if (baseIndent.isNotEmpty) {
        final modifiedText = '${oldText.substring(0, insertPos)}\n$baseIndent${oldText.substring(insertPos)}';
        _internalUpdate = true;
        super.value = TextEditingValue(
          text: modifiedText,
          selection: TextSelection.collapsed(offset: insertPos + 1 + baseIndent.length),
          composing: TextRange.empty,
        );
        _internalUpdate = false;
        pushUndoState();
        return;
      }
    }

    // 3. Auto-close opening brackets & quotes
    if (autoCloseBrackets) {
      if (oldSel.isValid &&
          oldSel.isCollapsed &&
          newSel.isValid &&
          newSel.isCollapsed &&
          newText.length == oldText.length + 1 &&
          newSel.baseOffset == oldSel.baseOffset + 1) {
        final insertPos = oldSel.baseOffset;
        final insertedChar = newText[insertPos];

        const bracketPairs = {
          '(': ')',
          '{': '}',
          '[': ']',
          '"': '"',
          "'": "'",
        };

        const closingChars = {')', '}', ']', '"', '\'', ';'};

        // Overtyping: skip over closing character
        if (closingChars.contains(insertedChar) &&
            insertPos < oldText.length &&
            oldText[insertPos] == insertedChar) {
          _internalUpdate = true;
          super.value = TextEditingValue(
            text: oldText,
            selection: TextSelection.collapsed(offset: insertPos + 1),
            composing: TextRange.empty,
          );
          _internalUpdate = false;
          pushUndoState();
          return;
        }

        // Auto-close opening bracket
        if (bracketPairs.containsKey(insertedChar)) {
          final closingChar = bracketPairs[insertedChar]!;
          final modifiedText =
              '${newText.substring(0, insertPos + 1)}$closingChar${newText.substring(insertPos + 1)}';

          _internalUpdate = true;
          super.value = TextEditingValue(
            text: modifiedText,
            selection: TextSelection.collapsed(offset: insertPos + 1),
            composing: TextRange.empty,
          );
          _internalUpdate = false;
          pushUndoState();
          return;
        }
      }

      // Backspace between matching pairs: delete both
      if (oldSel.isValid &&
          oldSel.isCollapsed &&
          newSel.isValid &&
          newSel.isCollapsed &&
          newText.length == oldText.length - 1 &&
          newSel.baseOffset == oldSel.baseOffset - 1) {
        final delPos = newSel.baseOffset;
        if (delPos < oldText.length - 1 && delPos >= 0) {
          final deletedChar = oldText[delPos];
          final nextChar = oldText[delPos + 1];

          const matchingPairs = {
            '(': ')',
            '{': '}',
            '[': ']',
            '"': '"',
            "'": "'",
          };

          if (matchingPairs[deletedChar] == nextChar) {
            final modifiedText =
                '${newText.substring(0, delPos)}${newText.substring(delPos + 1)}';
            _internalUpdate = true;
            super.value = TextEditingValue(
              text: modifiedText,
              selection: TextSelection.collapsed(offset: delPos),
              composing: TextRange.empty,
            );
            _internalUpdate = false;
            pushUndoState();
            return;
          }
        }
      }
    }

    super.value = newValue;
    pushUndoState();
  }

  @override
  TextSpan buildTextSpan({
    required BuildContext context,
    TextStyle? style,
    required bool withComposing,
  }) {
    final baseStyle = style ?? const TextStyle(fontFamily: 'monospace');
    if (!enableSyntaxHighlighting) {
      return TextSpan(style: baseStyle, text: text);
    }
    return AdeshSyntax.highlight(text, baseStyle);
  }

  void insertSnippet(String snippet, {int cursorOffset = 0}) {
    final sel = selection;
    final offset = sel.isValid ? sel.baseOffset.clamp(0, text.length) : text.length;

    const bracketPairs = {
      '(': ')',
      '{': '}',
      '[': ']',
      '"': '"',
      "'": "'",
    };

    if (autoCloseBrackets && bracketPairs.containsKey(snippet)) {
      final closeChar = bracketPairs[snippet]!;
      if (sel.isValid && !sel.isCollapsed) {
        final start = sel.start.clamp(0, text.length);
        final end = sel.end.clamp(0, text.length);
        final selectedText = text.substring(start, end);
        final wrapped = '$snippet$selectedText$closeChar';
        final newText = '${text.substring(0, start)}$wrapped${text.substring(end)}';
        _internalUpdate = true;
        value = TextEditingValue(
          text: newText,
          selection: TextSelection.collapsed(offset: end + snippet.length),
          composing: TextRange.empty,
        );
        _internalUpdate = false;
        pushUndoState();
        return;
      } else {
        final newText = '${text.substring(0, offset)}$snippet$closeChar${text.substring(offset)}';
        _internalUpdate = true;
        value = TextEditingValue(
          text: newText,
          selection: TextSelection.collapsed(offset: offset + snippet.length),
          composing: TextRange.empty,
        );
        _internalUpdate = false;
        pushUndoState();
        return;
      }
    }

    final newText = '${text.substring(0, offset)}$snippet${text.substring(offset)}';
    final newCursor = (offset + snippet.length + cursorOffset).clamp(0, newText.length);

    _internalUpdate = true;
    value = TextEditingValue(
      text: newText,
      selection: TextSelection.collapsed(offset: newCursor),
      composing: TextRange.empty,
    );
    _internalUpdate = false;
    pushUndoState();
  }

  void applyCompletion(String prefix, String completionText, {int cursorOffset = 0}) {
    final sel = selection;
    final offset = sel.isValid ? sel.baseOffset.clamp(0, text.length) : text.length;
    final startOffset = (offset - prefix.length).clamp(0, text.length);

    final newText = '${text.substring(0, startOffset)}$completionText${text.substring(offset)}';
    final newCursor = (startOffset + completionText.length + cursorOffset).clamp(0, newText.length);

    _internalUpdate = true;
    value = TextEditingValue(
      text: newText,
      selection: TextSelection.collapsed(offset: newCursor),
      composing: TextRange.empty,
    );
    _internalUpdate = false;
    pushUndoState();
  }

  void handleAutoIndent() {
    final sel = selection;
    if (!sel.isValid) return;

    final offset = sel.baseOffset.clamp(0, text.length);
    final lineStart = offset == 0 ? 0 : text.lastIndexOf('\n', offset - 1) + 1;
    final currentLine = text.substring(lineStart, offset);

    final indentMatch = RegExp(r'^\s*').firstMatch(currentLine);
    String indent = indentMatch != null ? indentMatch.group(0) ?? '' : '';

    if (currentLine.trimRight().endsWith('{')) {
      indent += ' ' * tabSize;
    }

    final snippet = '\n$indent';
    final newText = '${text.substring(0, offset)}$snippet${text.substring(offset)}';
    _internalUpdate = true;
    value = TextEditingValue(
      text: newText,
      selection: TextSelection.collapsed(offset: offset + snippet.length),
      composing: TextRange.empty,
    );
    _internalUpdate = false;
    pushUndoState();
  }

  void moveToLineStart() {
    final sel = selection;
    final offset = sel.isValid ? sel.baseOffset.clamp(0, text.length) : 0;
    final lineStart = offset == 0 ? 0 : text.lastIndexOf('\n', offset - 1) + 1;
    _internalUpdate = true;
    value = value.copyWith(
      selection: TextSelection.collapsed(offset: lineStart),
      composing: TextRange.empty,
    );
    _internalUpdate = false;
  }

  void moveToLineEnd() {
    final sel = selection;
    final offset = sel.isValid ? sel.baseOffset.clamp(0, text.length) : 0;
    final nextNl = text.indexOf('\n', offset);
    final lineEnd = nextNl == -1 ? text.length : nextNl;
    _internalUpdate = true;
    value = value.copyWith(
      selection: TextSelection.collapsed(offset: lineEnd),
      composing: TextRange.empty,
    );
    _internalUpdate = false;
  }

  void handleAutoClose(String insertedChar) {
    if (!autoCloseBrackets) return;
    insertSnippet(insertedChar);
  }
}
