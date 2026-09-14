;;; adesh-mode.el --- Major mode for AdeshLang with LSP support -*- lexical-binding: t; -*-

;; Copyright (C) 2026 AdeshLang Team
;; Author: AdeshLang Team
;; Version: 0.2.0
;; Keywords: languages tools
;; URL: https://github.com/ajaytainwala-dev/mylang

;;; Commentary:
;; Major mode for editing AdeshLang files with syntax highlighting,
;; indentation, and LSP integration via lsp-mode or eglot.
;;
;; Features:
;; - Syntax highlighting for all AdeshLang keywords, types, and operators
;; - LSP integration (completion, diagnostics, hover, go-to-definition)
;; - Type-aware auto-completion (IntelliSense) via ALS
;; - Inline diagnostics (errors, warnings) like TypeScript/JavaScript
;; - Semantic highlighting for ownership/borrow states
;; - Inlay hints for types and ownership
;; - Code formatting
;;
;; Installation:
;; 1. Build ALS: `cargo build --release` in the als/ directory
;; 2. Add this file to your load-path
;; 3. Add to your init.el:
;;    (require 'adesh-mode)
;;    (add-to-list 'auto-mode-alist '("\\.adesh\\'" . adesh-mode))
;;
;; For LSP integration, install either `lsp-mode' or `eglot':
;; - With lsp-mode: (add-hook 'adesh-mode-hook #'lsp)
;; - With eglot: (add-hook 'adesh-mode-hook #'eglot-ensure)

;;; Code:

(defvar adesh-mode-syntax-table
  (let ((table (make-syntax-table)))
    ;; C-style comments
    (modify-syntax-entry ?/ ". 124b" table)
    (modify-syntax-entry ?* ". 23" table)
    (modify-syntax-entry ?\n "> b" table)
    ;; Strings
    (modify-syntax-entry ?\" "\"" table)
    (modify-syntax-entry ?\' "\"" table)
    ;; Extra characters in symbols
    (modify-syntax-entry ?_ "w" table)
    (modify-syntax-entry ?! "w" table)
    (modify-syntax-entry ?? "w" table)
    table)
  "Syntax table for `adesh-mode'.")

(defconst adesh-keywords
  '("let" "const" "readonly" "test" "fn" "class" "extend" "raw" "decorator"
    "if" "else" "elif" "while" "do" "for" "in" "return" "break" "continue"
    "try" "catch" "throw" "import" "export" "from" "as" "new" "this" "super"
    "true" "false" "null" "async" "await" "enum" "interface"
    "extends" "implements" "static" "abstract" "type" "match" "struct"
    "pub" "mut" "yield" "defer" "where" "is" "typeof" "sizeof" "alignof"
    "get" "set" "region" "unsafe" "sealed" "on"))

(defconst adesh-type-keywords
  '("int" "i8" "i16" "i32" "i64" "i128"
    "u8" "u16" "u32" "u64" "u128"
    "f32" "f64" "bool" "string" "char"
    "tuple" "set" "object" "map" "complex"
    "void" "any" "never" "unknown" "null"
    "float" "str"))

(defconst adesh-arc-keywords
  '("share" "strong" "weak"))

(defconst adesh-ownership-keywords
  '("owned" "borrowed" "shared" "unique" "noalias" "move" "copy"))

(defconst adesh-memory-keywords
  '("alloc" "free" "drop" "heap" "stack" "region" "arena"
    "sizeof" "alignof" "offsetof"))

(defconst adesh-unsafe-keywords
  '("unsafe" "panic" "unreachable"))

(defconst adesh-builtin-functions
  '("print" "println" "input" "len" "type" "range" "map" "filter" "reduce"
    "clock" "sleep" "str" "max" "min" "assert" "assert_eq" "assert_ne"))

(defvar adesh-font-lock-keywords
  `((,(regexp-opt adesh-keywords 'words) . font-lock-keyword-face)
    (,(regexp-opt adesh-ownership-keywords 'words) . font-lock-builtin-face)
    (,(regexp-opt adesh-arc-keywords 'words) . font-lock-keyword-face)
    (,(regexp-opt adesh-memory-keywords 'words) . font-lock-constant-face)
    (,(regexp-opt adesh-unsafe-keywords 'words) . font-lock-warning-face)
    (,(regexp-opt adesh-type-keywords 'words) . font-lock-type-face)
    (,(regexp-opt adesh-builtin-functions 'words) . font-lock-builtin-face)
    ;; Borrow operators
    ("&\\(mut\\)?\\>" . font-lock-type-face)
    ;; Numbers including typed literals
    ("\\b[0-9]+\\(\\.[0-9]+\\)?j\\b" . font-lock-constant-face)
    ("\\b[0-9]+\\(u8\\|u16\\|u32\\|u64\\|u128\\|i8\\|i16\\|i32\\|i64\\|i128\\|f32\\|f64\\)\\b" . font-lock-constant-face)
    ("\\b[0-9]+\\(\\.[0-9]+\\)?\\b" . font-lock-constant-face)
    ;; Function calls
    ("\\b\\([a-zA-Z_][a-zA-Z0-9_]*\\)\\s-*(" 1 font-lock-function-name-face)
    ;; Class/struct/enum names (capitalized identifiers)
    ("\\b\\([A-Z][a-zA-Z0-9_]*\\)\\b" 1 font-lock-type-face)
    ;; Decorators
    ("@[a-zA-Z_][a-zA-Z0-9_]*" . font-lock-preprocessor-face))
  "Font lock keywords for `adesh-mode'.")

;;;###autoload
(define-derived-mode adesh-mode prog-mode "Adesh"
  "Major mode for editing AdeshLang files.

This mode provides syntax highlighting, indentation, and integrates
with LSP servers (ALS) for completion, diagnostics, and more.

\\{adesh-mode-map}"
  :syntax-table adesh-mode-syntax-table
  (setq-local comment-start "//")
  (setq-local comment-end "")
  (setq-local comment-start-skip "//+\\s-*")
  (setq-local comment-multi-line t)
  (setq-local font-lock-defaults '(adesh-font-lock-keywords nil t))
  (setq-local indent-tabs-mode nil)
  (setq-local tab-width 4)
  (setq-local indent-line-function 'adesh-indent-line)
  (setq-local electric-indent-chars '(?\n ?\} ?\) ?\]))
  (setq-local require-final-newline t)
  (setq-local imenu-generic-expression
        `(("Functions" "^\\s-*fn\\s-+\\([a-zA-Z_][a-zA-Z0-9_]*\\)" 1)
          ("Classes" "^\\s-*class\\s-+\\([a-zA-Z_][a-zA-Z0-9_]*\\)" 1)
          ("Structs" "^\\s-*struct\\s-+\\([a-zA-Z_][a-zA-Z0-9_]*\\)" 1)
          ("Enums" "^\\s-*enum\\s-+\\([a-zA-Z_][a-zA-Z0-9_]*\\)" 1)
          ("Interfaces" "^\\s-*interface\\s-+\\([a-zA-Z_][a-zA-Z0-9_]*\\)" 1)
          ("Constants" "^\\s-*const\\s-+\\([a-zA-Z_][a-zA-Z0-9_]*\\)" 1)))

  ;; Auto-start LSP if available
  (when (and (fboundp 'lsp) (featurep 'lsp-mode))
    (lsp))
  (when (and (fboundp 'eglot-ensure) (featurep 'eglot))
    (eglot-ensure)))

;; Simple indentation function
(defun adesh-indent-line ()
  "Indent the current line for Adesh mode."
  (interactive)
  (let ((indent-col 0))
    (save-excursion
      (beginning-of-line)
      (let ((prev-line (line-number-at-pos)))
        (when (> prev-line 1)
          (forward-line -1)
          (setq indent-col (current-indentation))
          ;; Increase indent after opening braces, certain keywords
          (end-of-line)
          (when (or (search-backward-regexp "{\\s-*$" (line-beginning-position) t)
                    (search-backward-regexp "\\b\\(if\\|else\\|elif\\|while\\|for\\|fn\\|class\\|struct\\|enum\\|interface\\|try\\|catch\\|match\\|region\\|unsafe\\)\\b" (line-beginning-position) t))
            (setq indent-col (+ indent-col 4))))))
    (save-excursion
      (beginning-of-line)
      (delete-horizontal-space)
      (indent-to indent-col))
    (when (< (current-column) indent-col)
      (move-to-column indent-col))))

;; LSP integration with lsp-mode
(defvar adesh-lsp-server-command '("als")
  "Command to start the Adesh Language Server (ALS).
Set this to the absolute path of the ALS binary if it's not in PATH.
Example: (setq adesh-lsp-server-command '(\"/path/to/AdeshLang/als/target/release/als\"))")

(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection adesh-lsp-server-command)
    :major-modes '(adesh-mode)
    :server-id 'als
    :initialized-fn (lambda (workspace)
                      (with-lsp-workspace workspace
                        (lsp--set-configuration
                         `(:adesh (:format (:indentSize 4 :useTabs :json-false :enable t)
                                  :borrowChecker (:enabled t :showOwnershipHints t :highlightBorrows t)
                                  :unsafe (:warnOnUsage t)
                                  :inlayHints (:enabled t :showOwnership t :showBorrowState :json-false)
                                  :completion (:enabled t :includeKeywords t :includeBuiltins t)
                                  :diagnostics (:enabled t :refreshOnSave t)
                                  :typeAssistance (:enabled t :showInferredTypes t :strictMode :json-false)
                                  :server (:enabled t :timeout 30000))))))))

;; LSP integration with eglot
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               `((adesh-mode) . ,adesh-lsp-server-command)))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.adesh\\'" . adesh-mode))
(add-to-list 'auto-mode-alist '("\\.vy\\'" . adesh-mode))

;;; Adesh Package Manifest (ADL) Mode

(defvar adl-mode-syntax-table
  (let ((table (make-syntax-table)))
    ;; // and # line comments, /* */ block comments
    (modify-syntax-entry ?/ ". 124b" table)
    (modify-syntax-entry ?* ". 23" table)
    (modify-syntax-entry ?# "<" table)
    (modify-syntax-entry ?\n ">" table)
    ;; Strings
    (modify-syntax-entry ?\" "\"" table)
    ;; Extra characters in symbols
    (modify-syntax-entry ?_ "w" table)
    (modify-syntax-entry ?- "w" table)
    table)
  "Syntax table for `adl-mode'.")

(defconst adl-section-keywords
  '("project" "compiler" "dependencies" "scripts" "workspace"))

(defconst adl-lock-keywords
  '("lock" "confidential"))

(defconst adl-decl-keywords
  '("let" "const" "import" "if" "else"))

(defconst adl-typed-value-keywords
  '("path" "url" "duration" "version"))

(defvar adl-font-lock-keywords
  `((,(regexp-opt adl-section-keywords 'words) . font-lock-type-face)
    (,(regexp-opt adl-lock-keywords 'words) . font-lock-keyword-face)
    (,(regexp-opt adl-decl-keywords 'words) . font-lock-keyword-face)
    (,(regexp-opt adl-typed-value-keywords 'words) . font-lock-builtin-face)
    ;; Section headers [section]
    ("^\\s-*\\[\\([a-zA-Z_-]+\\)\\]" 1 font-lock-preprocessor-face)
    ;; Boolean and null
    ("\\b\\(true\\|false\\|null\\)\\b" . font-lock-constant-face)
    ;; Version requirements
    ("\\(\\^\\|~\\|>=\\|<=\\|>\\|<\\|=?\\)\\s-*\\d+\\.\\d+\\.\\d+" . font-lock-constant-face)
    ;; Strings
    ("\"[^\"]*\"" . font-lock-string-face)
    ;; Numbers
    ("\\b-?[0-9]+\\(\\.[0-9]+\\)?\\([eE][+-]?[0-9]+\\)?\\b" . font-lock-constant-face)
    ;; Property keys (key =)
    ("^\\s-*\\([a-zA-Z_][a-zA-Z0-9_-]*\\)\\s-*=" 1 font-lock-variable-name-face)
    ;; Object keys (key =)
    ("{\\s-*\\([a-zA-Z_][a-zA-Z0-9_-]*\\)\\s-*=" 1 font-lock-variable-name-face))
  "Font lock keywords for `adl-mode'.")

;;;###autoload
(define-derived-mode adl-mode prog-mode "ADL"
  "Major mode for editing Adesh Package Manifest (ADL) files.

ADL is the configuration format used by the Adesh package manager.
Files include `adesh.adl' (project manifest) and `adesh.lock.adl' (lockfile).

This mode provides syntax highlighting for sections, key-value pairs,
version requirements, strings, and comments.

\\{adl-mode-map}"
  :syntax-table adl-mode-syntax-table
  (setq-local comment-start "//")
  (setq-local comment-end "")
  (setq-local comment-start-skip "//+\\s-*")
  (setq-local comment-multi-line t)
  (setq-local font-lock-defaults '(adl-font-lock-keywords nil t))
  (setq-local indent-tabs-mode nil)
  (setq-local tab-width 2)
  (setq-local indent-line-function 'adl-indent-line)
  (setq-local require-final-newline t)
  (setq-local imenu-generic-expression
        `(("Sections" "^\\s-*\\[\\([a-zA-Z_-]+\\)\\]" 1)
          ("Lock Block" "^\\s-*\\(lock\\)\\s-*{" 1)
          ("Scripts" "^\\s-*\\(const\\|let\\)\\s-+\\([a-zA-Z_][a-zA-Z0-9_]*\\)" 2))))

;; Simple indentation function for ADL
(defun adl-indent-line ()
  "Indent the current line for ADL mode."
  (interactive)
  (let ((indent-col 0))
    (save-excursion
      (beginning-of-line)
      (let ((prev-line (line-number-at-pos)))
        (when (> prev-line 1)
          (forward-line -1)
          (setq indent-col (current-indentation))
          ;; Increase indent after opening braces or brackets
          (end-of-line)
          (when (or (search-backward-regexp "{\\s-*$" (line-beginning-position) t)
                    (search-backward-regexp "\\[\\s-*$" (line-beginning-position) t)
                    (search-backward-regexp "\\b\\(lock\\|if\\|else\\)\\b" (line-beginning-position) t))
            (setq indent-col (+ indent-col 2))))))
    (save-excursion
      (beginning-of-line)
      (delete-horizontal-space)
      (indent-to indent-col))
    (when (< (current-column) indent-col)
      (move-to-column indent-col))))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.adl\\'" . adl-mode))
(add-to-list 'auto-mode-alist '("\\'adesh\\.adl\\'" . adl-mode))
(add-to-list 'auto-mode-alist '("\\'adesh\\.lock\\.adl\\'" . adl-mode))

(provide 'adl-mode)

(provide 'adesh-mode)
;;; adesh-mode.el ends here
