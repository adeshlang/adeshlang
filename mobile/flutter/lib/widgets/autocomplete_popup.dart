import 'package:flutter/material.dart';
import '../theme/app_theme.dart';

class AutocompleteSuggestion {
  final String label;
  final String insertText;
  final String detail;
  final String type;

  AutocompleteSuggestion({
    required this.label,
    required this.insertText,
    required this.detail,
    required this.type,
  });
}

class AutocompletePopup extends StatelessWidget {
  final List<AutocompleteSuggestion> suggestions;
  final Function(AutocompleteSuggestion) onSelect;

  const AutocompletePopup({
    super.key,
    required this.suggestions,
    required this.onSelect,
  });

  @override
  Widget build(BuildContext context) {
    if (suggestions.isEmpty) return const SizedBox.shrink();

    return Card(
      color: AppTheme.surface,
      elevation: 6,
      margin: EdgeInsets.zero,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(8),
        side: const BorderSide(color: AppTheme.border, width: 1),
      ),
      child: Container(
        constraints: const BoxConstraints(maxHeight: 160, maxWidth: 240),
        child: ListView.builder(
          shrinkWrap: true,
          itemCount: suggestions.length,
          itemBuilder: (context, index) {
            final item = suggestions[index];
            IconData iconData = Icons.code_rounded;
            Color iconColor = AppTheme.primary;

            if (item.type == 'function') {
              iconData = Icons.functions_rounded;
              iconColor = AppTheme.accent;
            } else if (item.type == 'snippet') {
              iconData = Icons.auto_awesome_rounded;
              iconColor = AppTheme.secondary;
            }

            return ListTile(
              dense: true,
              visualDensity: VisualDensity.compact,
              leading: Icon(iconData, size: 16, color: iconColor),
              title: Text(
                item.label,
                style: const TextStyle(
                  color: AppTheme.textPrimary,
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                  fontFamily: 'monospace',
                ),
              ),
              subtitle: Text(
                item.detail,
                style: const TextStyle(color: AppTheme.textSecondary, fontSize: 11),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
              onTap: () => onSelect(item),
            );
          },
        ),
      ),
    );
  }
}
