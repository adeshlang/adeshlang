class EditorFile {
  String name;
  String path;
  String content;
  bool isModified;
  DateTime lastModified;

  EditorFile({
    required this.name,
    required this.path,
    required this.content,
    this.isModified = false,
    DateTime? lastModified,
  }) : lastModified = lastModified ?? DateTime.now();

  EditorFile copyWith({
    String? name,
    String? path,
    String? content,
    bool? isModified,
    DateTime? lastModified,
  }) {
    return EditorFile(
      name: name ?? this.name,
      path: path ?? this.path,
      content: content ?? this.content,
      isModified: isModified ?? this.isModified,
      lastModified: lastModified ?? this.lastModified,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'name': name,
      'path': path,
      'content': content,
      'lastModified': lastModified.toIso8601String(),
    };
  }

  factory EditorFile.fromJson(Map<String, dynamic> json) {
    return EditorFile(
      name: json['name'] as String? ?? 'untitled.adesh',
      path: json['path'] as String? ?? '',
      content: json['content'] as String? ?? '',
      lastModified: json['lastModified'] != null
          ? DateTime.tryParse(json['lastModified'] as String) ?? DateTime.now()
          : DateTime.now(),
    );
  }
}
