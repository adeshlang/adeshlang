import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:url_launcher/url_launcher.dart';

import '../models/app_settings.dart';
import '../services/settings_service.dart';
import '../theme/app_theme.dart';

class SettingsScreen extends StatefulWidget {
  final AppSettings settings;
  final Function(AppSettings) onSave;

  const SettingsScreen({
    super.key,
    required this.settings,
    required this.onSave,
  });

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  late AppSettings _settings;
  final SettingsService _settingsService = SettingsService();

  @override
  void initState() {
    super.initState();
    _settings = widget.settings;
  }

  void _notifySave() {
    _settingsService.saveSettings(_settings);
    widget.onSave(_settings);
  }

  Future<void> _launchUrlHelper(String urlString) async {
    final uri = Uri.parse(urlString);
    try {
      final launched = await launchUrl(
        uri,
        mode: LaunchMode.externalApplication,
      );
      if (!launched) {
        await Clipboard.setData(ClipboardData(text: urlString));
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Could not open browser. Copied link: $urlString')),
          );
        }
      }
    } catch (_) {
      try {
        await launchUrl(uri, mode: LaunchMode.platformDefault);
      } catch (_) {
        await Clipboard.setData(ClipboardData(text: urlString));
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Copied link to clipboard: $urlString')),
          );
        }
      }
    }
  }

  void _showAboutDialog() {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: AppTheme.surface,
        title: Row(
          children: [
            Container(
              padding: const EdgeInsets.all(8),
              decoration: BoxDecoration(
                color: AppTheme.primary.withValues(alpha: 0.15),
                borderRadius: BorderRadius.circular(8),
              ),
              child: const Icon(Icons.code_rounded, color: AppTheme.primary, size: 24),
            ),
            const SizedBox(width: 12),
            const Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('Adesh Editor', style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                Text('Version 0.1.0 (Build 1)', style: TextStyle(fontSize: 12, color: AppTheme.textSecondary)),
              ],
            ),
          ],
        ),
        content: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              const Text(
                'Adesh Editor is a high-performance native mobile IDE and runtime environment for the Adesh programming language.',
                style: TextStyle(fontSize: 13, height: 1.4),
              ),
              const SizedBox(height: 16),
              _buildInfoRow('Language Version', 'AdeshLang v0.3.0'),
              _buildInfoRow('Engine', 'Native Rust Interpreter'),
              _buildInfoRow('Backend', 'ExecutionBackend::Interpreter'),
              _buildInfoRow('Bridge', 'C-ABI FFI (Zero Overhead)'),
              _buildInfoRow('Target Arch', 'ARM64-v8a & x86_64'),
              const SizedBox(height: 16),
              const Text(
                '© 2026 AdeshLang Developers. All rights reserved.',
                style: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
              ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Close'),
          ),
        ],
      ),
    );
  }

  Widget _buildInfoRow(String title, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(title, style: const TextStyle(fontSize: 12, color: AppTheme.textSecondary)),
          Text(value, style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w600, color: AppTheme.textPrimary)),
        ],
      ),
    );
  }

  void _showLicenseDialog() {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: AppTheme.surface,
        title: const Row(
          children: [
            Icon(Icons.gavel_rounded, color: AppTheme.secondary, size: 22),
            SizedBox(width: 8),
            Text('MIT License', style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
          ],
        ),
        content: SizedBox(
          width: double.maxFinite,
          child: SingleChildScrollView(
            child: Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: AppTheme.editorBackground,
                borderRadius: BorderRadius.circular(8),
                border: Border.all(color: AppTheme.border),
              ),
              child: const Text(
                '''MIT License

Copyright (c) 2026 AdeshLang Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.''',
                style: TextStyle(fontFamily: 'monospace', fontSize: 11, height: 1.4, color: AppTheme.textPrimary),
              ),
            ),
          ),
        ),
        actions: [
          TextButton.icon(
            icon: const Icon(Icons.copy_rounded, size: 16),
            label: const Text('Copy License'),
            onPressed: () async {
              await Clipboard.setData(const ClipboardData(
                text: '''MIT License\n\nCopyright (c) 2026 AdeshLang Contributors\n...''',
              ));
              if (context.mounted) {
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(content: Text('License text copied to clipboard')),
                );
              }
            },
          ),
          ElevatedButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Close'),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Editor Settings'),
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          const Text(
            'EDITOR APPEARANCE',
            style: TextStyle(
              color: AppTheme.textSecondary,
              fontSize: 12,
              fontWeight: FontWeight.bold,
              letterSpacing: 1.2,
            ),
          ),
          const SizedBox(height: 10),

          Card(
            color: AppTheme.surface,
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      const Text('Editor Font Size', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 14)),
                      Text('${_settings.fontSize.toInt()} px', style: const TextStyle(color: AppTheme.primary, fontWeight: FontWeight.bold)),
                    ],
                  ),
                  Slider(
                    value: _settings.fontSize,
                    min: 10.0,
                    max: 28.0,
                    divisions: 18,
                    label: '${_settings.fontSize.toInt()} px',
                    onChanged: (val) {
                      setState(() {
                        _settings.fontSize = val;
                      });
                      _notifySave();
                    },
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 12),

          Card(
            color: AppTheme.surface,
            child: ListTile(
              title: const Text('Tab Size'),
              subtitle: const Text('Spaces per tab indentation'),
              trailing: DropdownButton<int>(
                value: _settings.tabSize,
                dropdownColor: AppTheme.surface,
                items: const [
                  DropdownMenuItem(value: 2, child: Text('2 spaces')),
                  DropdownMenuItem(value: 4, child: Text('4 spaces')),
                ],
                onChanged: (val) {
                  if (val != null) {
                    setState(() => _settings.tabSize = val);
                    _notifySave();
                  }
                },
              ),
            ),
          ),
          const SizedBox(height: 12),

          Card(
            color: AppTheme.surface,
            child: ListTile(
              title: const Text('Code Font Family'),
              subtitle: Text(_settings.fontFamily, style: const TextStyle(color: AppTheme.primary, fontSize: 12)),
              trailing: DropdownButton<String>(
                value: _settings.fontFamily,
                dropdownColor: AppTheme.surface,
                items: const [
                  DropdownMenuItem(value: 'JetBrains Mono', child: Text('JetBrains Mono')),
                  DropdownMenuItem(value: 'Fira Code', child: Text('Fira Code')),
                  DropdownMenuItem(value: 'Roboto Mono', child: Text('Roboto Mono')),
                  DropdownMenuItem(value: 'Courier Prime', child: Text('Courier Prime')),
                ],
                onChanged: (val) {
                  if (val != null) {
                    setState(() => _settings.fontFamily = val);
                    _notifySave();
                  }
                },
              ),
            ),
          ),
          const SizedBox(height: 20),

          const Text(
            'EDITOR FEATURES',
            style: TextStyle(
              color: AppTheme.textSecondary,
              fontSize: 12,
              fontWeight: FontWeight.bold,
              letterSpacing: 1.2,
            ),
          ),
          const SizedBox(height: 10),

          Card(
            color: AppTheme.surface,
            child: Column(
              children: [
                SwitchListTile(
                  title: const Text('Show Line Numbers'),
                  subtitle: const Text('Display line gutter in code editor'),
                  value: _settings.showLineNumbers,
                  onChanged: (val) {
                    setState(() => _settings.showLineNumbers = val);
                    _notifySave();
                  },
                ),
                const Divider(height: 1),
                SwitchListTile(
                  title: const Text('Syntax Highlighting'),
                  subtitle: const Text('Colorize keywords, strings, types, and numbers'),
                  value: _settings.enableSyntaxHighlighting,
                  onChanged: (val) {
                    setState(() => _settings.enableSyntaxHighlighting = val);
                    _notifySave();
                  },
                ),
                const Divider(height: 1),
                SwitchListTile(
                  title: const Text('Auto-Close Brackets & Quotes'),
                  subtitle: const Text('Automatically close (), {}, [], "", and \'\''),
                  value: _settings.autoCloseBrackets,
                  onChanged: (val) {
                    setState(() => _settings.autoCloseBrackets = val);
                    _notifySave();
                  },
                ),
                const Divider(height: 1),
                SwitchListTile(
                  title: const Text('Autocomplete Suggestions'),
                  subtitle: const Text('Show intelligent keyword and function completions'),
                  value: _settings.enableAutocomplete,
                  onChanged: (val) {
                    setState(() => _settings.enableAutocomplete = val);
                    _notifySave();
                  },
                ),
                const Divider(height: 1),
                SwitchListTile(
                  title: const Text('Autosave'),
                  subtitle: const Text('Automatically save file changes while typing'),
                  value: _settings.enableAutosave,
                  onChanged: (val) {
                    setState(() => _settings.enableAutosave = val);
                    _notifySave();
                  },
                ),
                const Divider(height: 1),
                SwitchListTile(
                  title: const Text('Auto-Format Code'),
                  subtitle: const Text('Automatically format source code before save / run'),
                  value: _settings.enableAutoFormat,
                  onChanged: (val) {
                    setState(() => _settings.enableAutoFormat = val);
                    _notifySave();
                  },
                ),
                const Divider(height: 1),
                SwitchListTile(
                  title: const Text('Auto-open Console'),
                  subtitle: const Text('Expand output drawer automatically when code runs'),
                  value: _settings.autoOpenConsole,
                  onChanged: (val) {
                    setState(() => _settings.autoOpenConsole = val);
                    _notifySave();
                  },
                ),
              ],
            ),
          ),

          const SizedBox(height: 20),
          const Text(
            'ABOUT & DOCUMENTATION',
            style: TextStyle(
              color: AppTheme.textSecondary,
              fontSize: 12,
              fontWeight: FontWeight.bold,
              letterSpacing: 1.2,
            ),
          ),
          const SizedBox(height: 10),

          Card(
            color: AppTheme.surface,
            child: Column(
              children: [
                ListTile(
                  leading: const Icon(Icons.info_outline_rounded, color: AppTheme.primary),
                  title: const Text('About Adesh Editor'),
                  subtitle: const Text('Version 0.1.0 • Engine details & Architecture'),
                  trailing: const Icon(Icons.chevron_right_rounded),
                  onTap: _showAboutDialog,
                ),
                const Divider(height: 1),
                ListTile(
                  leading: const Icon(Icons.description_outlined, color: AppTheme.secondary),
                  title: const Text('Documentation Website'),
                  subtitle: const Text('adeshlang.org / language guides'),
                  trailing: const Icon(Icons.open_in_new_rounded, size: 18),
                  onTap: () => _launchUrlHelper('https://adeshlang.org'),
                ),
                const Divider(height: 1),
                ListTile(
                  leading: const Icon(Icons.code_rounded, color: AppTheme.accent),
                  title: const Text('GitHub Repository'),
                  subtitle: const Text('github.com/adeshlang/adeshlang'),
                  trailing: const Icon(Icons.open_in_new_rounded, size: 18),
                  onTap: () => _launchUrlHelper('https://github.com/adeshlang/adeshlang'),
                ),
                const Divider(height: 1),
                ListTile(
                  leading: const Icon(Icons.gavel_rounded, color: AppTheme.warning),
                  title: const Text('License'),
                  subtitle: const Text('MIT Open Source License'),
                  trailing: const Icon(Icons.chevron_right_rounded),
                  onTap: _showLicenseDialog,
                ),
              ],
            ),
          ),

          const SizedBox(height: 20),
          const Center(
            child: Text(
              'Adesh Editor v0.1.0 • Built with Flutter & Rust FFI',
              style: TextStyle(color: AppTheme.textSecondary, fontSize: 11),
            ),
          ),
          const SizedBox(height: 10),
        ],
      ),
    );
  }
}
