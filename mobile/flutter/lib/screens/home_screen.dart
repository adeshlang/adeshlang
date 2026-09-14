import 'package:flutter/material.dart';
import 'package:file_picker/file_picker.dart';
import 'dart:io';

import '../models/app_settings.dart';
import '../models/editor_file.dart';
import '../services/file_service.dart';
import '../services/settings_service.dart';
import '../theme/app_theme.dart';
import 'editor_screen.dart';
import 'examples_screen.dart';
import 'learn_screen.dart';
import 'settings_screen.dart';

class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key});

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  final FileService _fileService = FileService();
  final SettingsService _settingsService = SettingsService();

  List<EditorFile> _recentFiles = [];
  List<EditorFile> _allFiles = [];
  AppSettings _settings = AppSettings();
  bool _isLoading = true;

  @override
  void initState() {
    super.initState();
    _loadData();
  }

  Future<void> _loadData() async {
    setState(() => _isLoading = true);
    _settings = await _settingsService.loadSettings();
    _recentFiles = await _fileService.getRecentFiles();
    _allFiles = await _fileService.getAllFiles();
    setState(() => _isLoading = false);
  }

  Future<void> _createNewFile() async {
    final TextEditingController nameController = TextEditingController(text: 'main.adesh');
    final result = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Create New Adesh File'),
        content: TextField(
          controller: nameController,
          autofocus: true,
          decoration: const InputDecoration(
            labelText: 'File Name',
            hintText: 'example.adesh',
            border: OutlineInputBorder(),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          ElevatedButton(
            onPressed: () => Navigator.pop(context, nameController.text.trim()),
            child: const Text('Create'),
          ),
        ],
      ),
    );

    if (result != null && result.isNotEmpty) {
      final newFile = await _fileService.createNewFile(
        name: result,
        content: '// AdeshLang Script\n\nfn main() {\n    print("Hello, AdeshLang!");\n}\n',
      );
      if (!mounted) return;
      await Navigator.push(
        context,
        MaterialPageRoute(
          builder: (context) => EditorScreen(
            file: newFile,
            settings: _settings,
          ),
        ),
      );
      _loadData();
    }
  }

  Future<void> _pickDeviceFile() async {
    try {
      final result = await FilePicker.platform.pickFiles(
        type: FileType.any,
      );

      if (result != null && result.files.isNotEmpty && result.files.single.path != null) {
        final path = result.files.single.path!;
        final file = File(path);
        final content = await file.readAsString();
        final name = result.files.single.name;

        final editorFile = EditorFile(
          name: name,
          path: path,
          content: content,
        );

        await _fileService.addToRecent(editorFile);

        if (!mounted) return;
        await Navigator.push(
          context,
          MaterialPageRoute(
            builder: (context) => EditorScreen(
              file: editorFile,
              settings: _settings,
            ),
          ),
        );
        _loadData();
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Could not open file: $e'),
            backgroundColor: AppTheme.error,
          ),
        );
      }
    }
  }

  Future<void> _openExistingFile() async {
    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.surface,
      isScrollControlled: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (ctx) => SafeArea(
        child: Container(
          constraints: BoxConstraints(
            maxHeight: MediaQuery.of(ctx).size.height * 0.75,
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              // Drag handle & title
              Center(
                child: Container(
                  width: 36,
                  height: 4,
                  margin: const EdgeInsets.only(top: 12, bottom: 12),
                  decoration: BoxDecoration(
                    color: AppTheme.border,
                    borderRadius: BorderRadius.circular(2),
                  ),
                ),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
                child: Row(
                  children: [
                    const Icon(Icons.folder_open_rounded, color: AppTheme.primary, size: 20),
                    const SizedBox(width: 8),
                    const Text(
                      'Open File',
                      style: TextStyle(fontWeight: FontWeight.bold, fontSize: 16),
                    ),
                    const Spacer(),
                    IconButton(
                      icon: const Icon(Icons.close_rounded, size: 20),
                      onPressed: () => Navigator.pop(ctx),
                    ),
                  ],
                ),
              ),
              const Divider(height: 1),

              // Option: Browse device files
              ListTile(
                leading: Container(
                  padding: const EdgeInsets.all(8),
                  decoration: BoxDecoration(
                    color: AppTheme.primary.withValues(alpha: 0.15),
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: const Icon(Icons.file_open_rounded, color: AppTheme.primary, size: 20),
                ),
                title: const Text('Browse Device Storage', style: TextStyle(fontWeight: FontWeight.w600)),
                subtitle: const Text('Pick any .adesh or source file on device', style: TextStyle(fontSize: 12)),
                trailing: const Icon(Icons.chevron_right_rounded),
                onTap: () {
                  Navigator.pop(ctx);
                  _pickDeviceFile();
                },
              ),

              if (_allFiles.isNotEmpty) ...[
                const Divider(height: 1),
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 12, 16, 6),
                  child: Text(
                    'IN-APP FILES (${_allFiles.length})',
                    style: const TextStyle(
                      color: AppTheme.textSecondary,
                      fontSize: 11,
                      fontWeight: FontWeight.bold,
                      letterSpacing: 1.1,
                    ),
                  ),
                ),
                Flexible(
                  child: ListView.separated(
                    shrinkWrap: true,
                    padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
                    itemCount: _allFiles.length,
                    separatorBuilder: (c, i) => const SizedBox(height: 6),
                    itemBuilder: (c, index) {
                      final file = _allFiles[index];
                      return Material(
                        color: AppTheme.editorBackground,
                        borderRadius: BorderRadius.circular(8),
                        child: InkWell(
                          borderRadius: BorderRadius.circular(8),
                          onTap: () async {
                            Navigator.pop(ctx);
                            final content = await _fileService.getFileContent(file.path);
                            file.content = content;
                            if (!mounted) return;
                            await Navigator.push(
                              context,
                              MaterialPageRoute(
                                builder: (context) => EditorScreen(
                                  file: file,
                                  settings: _settings,
                                ),
                              ),
                            );
                            _loadData();
                          },
                          child: Padding(
                            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                            child: Row(
                              children: [
                                const Icon(Icons.code_rounded, color: AppTheme.secondary, size: 20),
                                const SizedBox(width: 10),
                                Expanded(
                                  child: Column(
                                    crossAxisAlignment: CrossAxisAlignment.start,
                                    children: [
                                      Text(
                                        file.name,
                                        style: const TextStyle(
                                          fontWeight: FontWeight.w600,
                                          fontSize: 13,
                                          color: AppTheme.textPrimary,
                                        ),
                                      ),
                                      Text(
                                        file.path,
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                        style: const TextStyle(fontSize: 10, color: AppTheme.textSecondary),
                                      ),
                                    ],
                                  ),
                                ),
                                const Icon(Icons.arrow_forward_ios_rounded, size: 12, color: AppTheme.textSecondary),
                              ],
                            ),
                          ),
                        ),
                      );
                    },
                  ),
                ),
              ],
              const SizedBox(height: 12),
            ],
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return DefaultTabController(
      length: 2,
      child: Scaffold(
        appBar: AppBar(
          title: const Row(
            children: [
              Icon(Icons.terminal_rounded, color: AppTheme.secondary),
              SizedBox(width: 8),
              Text('Adesh Editor'),
            ],
          ),
          bottom: const TabBar(
            tabs: [
              Tab(icon: Icon(Icons.history_rounded, size: 18), text: 'Recent'),
              Tab(icon: Icon(Icons.folder_rounded, size: 18), text: 'My Files'),
            ],
          ),
          actions: [
            IconButton(
              icon: const Icon(Icons.settings_rounded),
              tooltip: 'Settings',
              onPressed: () async {
                await Navigator.push(
                  context,
                  MaterialPageRoute(
                    builder: (context) => SettingsScreen(
                      settings: _settings,
                      onSave: (s) {
                        _settings = s;
                        _settingsService.saveSettings(s);
                      },
                    ),
                  ),
                );
              },
            ),
          ],
        ),
        body: _isLoading
            ? const Center(child: CircularProgressIndicator())
            : TabBarView(
                children: [
                  // ── Tab 1: Recent Files ──────────────────────────────
                  RefreshIndicator(
                    onRefresh: _loadData,
                    child: ListView(
                      padding: const EdgeInsets.all(16),
                      children: [
                        _buildHeaderBanner(),
                        const SizedBox(height: 20),

                        const Text(
                          'QUICK ACTIONS',
                          style: TextStyle(
                            color: AppTheme.textSecondary,
                            fontSize: 12,
                            fontWeight: FontWeight.bold,
                            letterSpacing: 1.2,
                          ),
                        ),
                        const SizedBox(height: 10),
                        _buildQuickActionsGrid(),
                        const SizedBox(height: 24),

                        Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            const Text(
                              'RECENT FILES',
                              style: TextStyle(
                                color: AppTheme.textSecondary,
                                fontSize: 12,
                                fontWeight: FontWeight.bold,
                                letterSpacing: 1.2,
                              ),
                            ),
                            if (_recentFiles.isNotEmpty)
                              Text(
                                '${_recentFiles.length} file(s)',
                                style: const TextStyle(color: AppTheme.textSecondary, fontSize: 12),
                              ),
                          ],
                        ),
                        const SizedBox(height: 10),

                        _recentFiles.isEmpty
                            ? _buildEmptyRecentCard()
                            : ListView.separated(
                                shrinkWrap: true,
                                physics: const NeverScrollableScrollPhysics(),
                                itemCount: _recentFiles.length,
                                separatorBuilder: (context, index) => const SizedBox(height: 8),
                                itemBuilder: (context, index) {
                                  final file = _recentFiles[index];
                                  return _buildRecentFileItem(file);
                                },
                              ),
                      ],
                    ),
                  ),

                  // ── Tab 2: My Files (all created .adesh files) ───────
                  RefreshIndicator(
                    onRefresh: _loadData,
                    child: _allFiles.isEmpty
                        ? _buildEmptyMyFilesCard()
                        : ListView.separated(
                            padding: const EdgeInsets.all(16),
                            itemCount: _allFiles.length,
                            separatorBuilder: (context, idx) => const SizedBox(height: 8),
                            itemBuilder: (context, index) {
                              final file = _allFiles[index];
                              return _buildMyFileItem(file);
                            },
                          ),
                  ),
                ],
              ),
        floatingActionButton: FloatingActionButton(
          onPressed: _createNewFile,
          tooltip: 'New File',
          backgroundColor: AppTheme.secondary,
          foregroundColor: Colors.black,
          child: const Icon(Icons.add_rounded),
        ),
      ),
    );
  }

  Widget _buildHeaderBanner() {
    return Container(
      padding: const EdgeInsets.all(20),
      decoration: BoxDecoration(
        gradient: const LinearGradient(
          colors: [Color(0xFF161B22), Color(0xFF0D1117)],
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
        ),
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: AppTheme.border, width: 1),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(
                  color: AppTheme.secondary.withValues(alpha: 0.15),
                  borderRadius: BorderRadius.circular(12),
                ),
                child: const Icon(Icons.code_rounded, color: AppTheme.secondary, size: 28),
              ),
              const SizedBox(width: 14),
              const Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'Adesh Editor',
                      style: TextStyle(
                        fontSize: 20,
                        fontWeight: FontWeight.bold,
                        color: AppTheme.textPrimary,
                      ),
                    ),
                    SizedBox(height: 2),
                    Text(
                      'High-Performance Mobile Code Editor & Runtime',
                      style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
                    ),
                  ],
                ),
              ),
            ],
          ),
          const SizedBox(height: 14),
          Wrap(
            spacing: 8,
            runSpacing: 6,
            children: [
              _buildBadge('v0.3.0', AppTheme.primary),
              _buildBadge('Interpreter Engine', AppTheme.secondary),
              _buildBadge('Zero-GC', AppTheme.accent),
            ],
          ),
        ],
      ),
    );
  }

  Widget _buildBadge(String label, Color color) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: color.withValues(alpha: 0.4), width: 1),
      ),
      child: Text(
        label,
        style: TextStyle(color: color, fontSize: 11, fontWeight: FontWeight.w600),
      ),
    );
  }

  Widget _buildQuickActionsGrid() {
    return GridView.count(
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      crossAxisCount: 2,
      crossAxisSpacing: 10,
      mainAxisSpacing: 10,
      childAspectRatio: 1.6,
      children: [
        _buildActionCard(
          icon: Icons.add_rounded,
          title: 'New File',
          subtitle: 'Create .adesh file',
          color: AppTheme.secondary,
          onTap: _createNewFile,
        ),
        _buildActionCard(
          icon: Icons.folder_open_rounded,
          title: 'Open File',
          subtitle: 'Browse local files',
          color: AppTheme.primary,
          onTap: _openExistingFile,
        ),
        _buildActionCard(
          icon: Icons.collections_bookmark_rounded,
          title: 'Examples',
          subtitle: 'Code samples',
          color: AppTheme.accent,
          onTap: () async {
            await Navigator.push(
              context,
              MaterialPageRoute(
                builder: (context) => ExamplesScreen(settings: _settings),
              ),
            );
            _loadData();
          },
        ),
        _buildActionCard(
          icon: Icons.school_rounded,
          title: 'Learn Adesh',
          subtitle: 'Syntax & Reference',
          color: AppTheme.warning,
          onTap: () {
            Navigator.push(
              context,
              MaterialPageRoute(builder: (context) => const LearnScreen()),
            );
          },
        ),
      ],
    );
  }

  Widget _buildActionCard({
    required IconData icon,
    required String title,
    required String subtitle,
    required Color color,
    required VoidCallback onTap,
  }) {
    return Card(
      color: AppTheme.surface,
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(12),
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(icon, color: color, size: 24),
              const SizedBox(height: 8),
              Text(
                title,
                style: const TextStyle(
                  fontWeight: FontWeight.bold,
                  fontSize: 14,
                  color: AppTheme.textPrimary,
                ),
              ),
              Text(
                subtitle,
                style: const TextStyle(fontSize: 11, color: AppTheme.textSecondary),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildEmptyRecentCard() {
    return Card(
      color: AppTheme.surface,
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          children: [
            const Icon(Icons.description_outlined, size: 36, color: AppTheme.textSecondary),
            const SizedBox(height: 8),
            const Text(
              'No recent files yet',
              style: TextStyle(fontWeight: FontWeight.bold, color: AppTheme.textPrimary),
            ),
            const SizedBox(height: 4),
            const Text(
              'Create or open a file to start coding',
              style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
            ),
            const SizedBox(height: 12),
            ElevatedButton.icon(
              onPressed: _createNewFile,
              icon: const Icon(Icons.add, size: 16),
              label: const Text('Create New File'),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildRecentFileItem(EditorFile file) {
    final lineCount = file.content.isNotEmpty ? file.content.split('\n').length : 0;
    final sizeText = lineCount > 0 ? '$lineCount line${lineCount == 1 ? '' : 's'}' : 'Empty file';

    return Card(
      color: AppTheme.surface,
      margin: EdgeInsets.zero,
      child: ListTile(
        leading: const Icon(Icons.insert_drive_file_rounded, color: AppTheme.primary),
        title: Text(
          file.name,
          style: const TextStyle(
            fontWeight: FontWeight.w600,
            fontSize: 14,
            color: AppTheme.textPrimary,
          ),
        ),
        subtitle: Text(
          sizeText,
          style: const TextStyle(fontSize: 11, color: AppTheme.textSecondary),
        ),
        trailing: IconButton(
          icon: const Icon(Icons.close_rounded, size: 18, color: AppTheme.textSecondary),
          tooltip: 'Remove from recents',
          onPressed: () async {
            await _fileService.removeFromRecent(file.path);
            _loadData();
          },
        ),
        onTap: () async {
          await Navigator.push(
            context,
            MaterialPageRoute(
              builder: (context) => EditorScreen(
                file: file,
                settings: _settings,
              ),
            ),
          );
          _loadData();
        },
      ),
    );
  }

  Widget _buildEmptyMyFilesCard() {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            const Icon(Icons.folder_open_rounded, size: 56, color: AppTheme.textSecondary),
            const SizedBox(height: 16),
            const Text(
              'No files yet',
              style: TextStyle(fontWeight: FontWeight.bold, fontSize: 16, color: AppTheme.textPrimary),
            ),
            const SizedBox(height: 8),
            const Text(
              'Create a new file to get started',
              style: TextStyle(fontSize: 13, color: AppTheme.textSecondary),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 20),
            ElevatedButton.icon(
              onPressed: _createNewFile,
              icon: const Icon(Icons.add, size: 18),
              label: const Text('Create New File'),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildMyFileItem(EditorFile file) {
    return Card(
      color: AppTheme.surface,
      margin: EdgeInsets.zero,
      child: ListTile(
        leading: const Icon(Icons.insert_drive_file_rounded, color: AppTheme.secondary),
        title: Text(
          file.name,
          style: const TextStyle(
            fontWeight: FontWeight.w600,
            fontSize: 14,
            color: AppTheme.textPrimary,
          ),
        ),
        subtitle: const Text(
          'Tap to open',
          style: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
        ),
        trailing: PopupMenuButton<String>(
          icon: const Icon(Icons.more_vert_rounded, size: 18, color: AppTheme.textSecondary),
          itemBuilder: (context) => [
            const PopupMenuItem(value: 'delete', child: Row(
              children: [Icon(Icons.delete_outline, color: Colors.red, size: 18), SizedBox(width: 8), Text('Delete', style: TextStyle(color: Colors.red))],
            )),
          ],
          onSelected: (value) async {
            if (value == 'delete') {
              final confirm = await showDialog<bool>(
                context: context,
                builder: (ctx) => AlertDialog(
                  title: const Text('Delete File'),
                  content: Text('Delete "${file.name}"? This cannot be undone.'),
                  actions: [
                    TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Cancel')),
                    TextButton(
                      onPressed: () => Navigator.pop(ctx, true),
                      style: TextButton.styleFrom(foregroundColor: Colors.red),
                      child: const Text('Delete'),
                    ),
                  ],
                ),
              );
              if (confirm == true) {
                await _fileService.deleteFile(file.path);
                _loadData();
              }
            }
          },
        ),
        onTap: () async {
          // Load content before opening
          final content = await _fileService.getFileContent(file.path);
          file.content = content;
          if (!mounted) return;
          await Navigator.push(
            context,
            MaterialPageRoute(
              builder: (context) => EditorScreen(
                file: file,
                settings: _settings,
              ),
            ),
          );
          _loadData();
        },
      ),
    );
  }
}
