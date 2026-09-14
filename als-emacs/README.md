# ALS for Emacs

AdeshLang Language Server support for Emacs using lsp-mode or eglot.

## Features

- Syntax highlighting
- Auto-completion (company-mode)
- Hover information (eldoc)
- Go-to-definition
- Find references
- Document symbols (imenu)
- Code formatting
- Diagnostics (flycheck)
- Semantic tokens
- Inlay hints

## Installation

### Option 1: Using lsp-mode (Recommended)

#### 1. Install Required Packages

Add to your `init.el` or `.emacs`:

```elisp
(use-package lsp-mode
  :ensure t
  :init
  (setq lsp-keymap-prefix "C-c l")
  :hook ((adesh-mode . lsp))
  :commands lsp)

(use-package lsp-ui
  :ensure t
  :commands lsp-ui-mode)

(use-package company
  :ensure t
  :config
  (global-company-mode))

(use-package flycheck
  :ensure t
  :config
  (global-flycheck-mode))
```

#### 2. Define Adesh Mode

Create `adesh-mode.el`:

```elisp
;;; adesh-mode.el --- Major mode for AdeshLang -*- lexical-binding: t; -*-

(defvar adesh-mode-syntax-table
  (let ((table (make-syntax-table)))
    ;; Comments
    (modify-syntax-entry ?/ ". 124b" table)
    (modify-syntax-entry ?* ". 23" table)
    (modify-syntax-entry ?\n "> b" table)
    table)
  "Syntax table for `adesh-mode'.")

(defconst adesh-keywords
  '("let" "const" "fn" "class" "if" "else" "elif" "while" "for" "in"
    "return" "break" "continue" "try" "catch" "throw"
    "import" "export" "from" "as" "new" "this" "super"
    "true" "false" "null" "async" "await" "enum" "interface"
    "extends" "implements" "static" "abstract" "type" "match"
    "pub" "mut" "yield" "defer" "where" "is" "typeof" "set" "get"))

(defconst adesh-ownership-keywords
  '("owned" "borrowed" "shared" "unique" "noalias" "move" "copy"))

(defconst adesh-memory-keywords
  '("alloc" "free" "drop" "heap" "stack" "region" "arena"
    "sizeof" "alignof" "offsetof"))

(defconst adesh-unsafe-keywords
  '("unsafe" "panic" "unreachable"))

(defvar adesh-font-lock-keywords
  `((,(regexp-opt adesh-keywords 'words) . font-lock-keyword-face)
    (,(regexp-opt adesh-ownership-keywords 'words) . font-lock-builtin-face)
    (,(regexp-opt adesh-memory-keywords 'words) . font-lock-constant-face)
    (,(regexp-opt adesh-unsafe-keywords 'words) . font-lock-warning-face)
    ("&\\(mut\\)?\\>" . font-lock-type-face)
    ("\\b[0-9]+\\(\\.[0-9]+\\)?\\b" . font-lock-constant-face)
    ("\\b\\([a-zA-Z_][a-zA-Z0-9_]*\\)\\s-*(" 1 font-lock-function-name-face)))

;;;###autoload
(define-derived-mode adesh-mode prog-mode "Adesh"
  "Major mode for editing AdeshLang files."
  :syntax-table adesh-mode-syntax-table
  (setq-local comment-start "//")
  (setq-local comment-end "")
  (setq-local font-lock-defaults '(adesh-font-lock-keywords))
  (setq-local indent-tabs-mode nil)
  (setq-local tab-width 4))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.adesh\\'" . adesh-mode))
(add-to-list 'auto-mode-alist '("\\.vy\\'" . adesh-mode))

(provide 'adesh-mode)
;;; adesh-mode.el ends here
```

Load it in your `init.el`:

```elisp
(load "path/to/adesh-mode.el")
```

#### 3. Register ALS with lsp-mode

Add to your `init.el`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection '("/path/to/als/target/release/als"))
    :major-modes '(adesh-mode)
    :server-id 'als
    :initialization-options
    '((format . ((indentSize . 4)
                 (useTabs . :json-false)
                 (enable . t)))
      (borrowChecker . ((enabled . t)
                        (showOwnershipHints . t)
                        (highlightBorrows . t)))
      (unsafe . ((warnOnUsage . t)))
      (inlayHints . ((enabled . t)
                     (showOwnership . t)
                     (showBorrowState . :json-false)))))))
```

**Important**: Replace `/path/to/als/target/release/als` with the actual path.

### Option 2: Using eglot

#### 1. Install eglot

```elisp
(use-package eglot
  :ensure t
  :hook (adesh-mode . eglot-ensure))
```

#### 2. Register ALS

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(adesh-mode . ("/path/to/als/target/release/als"))))
```

## Usage

### Key Bindings (lsp-mode)

With `lsp-keymap-prefix` set to `C-c l`:

- **Find definition**: `M-.` or `C-c l g d`
- **Find references**: `C-c l g r`
- **Hover**: `C-c l h h` or `K` (evil-mode)
- **Completion**: `M-TAB` or configured company bindings
- **Format**: `C-c l = =`
- **Rename**: `C-c l r r`
- **Code actions**: `C-c l a a`

### Key Bindings (eglot)

- **Find definition**: `M-.`
- **Find references**: `M-?`
- **Hover**: `eldoc` shows automatically
- **Completion**: `M-TAB` or `C-M-i`
- **Format**: `C-c C-f` (custom binding needed)
- **Rename**: `C-c r` (custom binding needed)

### Custom Key Bindings

Add to your `init.el`:

```elisp
(with-eval-after-load 'adesh-mode
  (define-key adesh-mode-map (kbd "C-c C-f") 'lsp-format-buffer)  ; or eglot-format-buffer
  (define-key adesh-mode-map (kbd "C-c r") 'lsp-rename))          ; or eglot-rename
```

## Configuration

### lsp-mode Settings

```elisp
(setq lsp-adesh-format-indent-size 4
      lsp-adesh-format-use-tabs nil
      lsp-adesh-borrow-checker-enabled t
      lsp-adesh-unsafe-warn-on-usage t
      lsp-adesh-inlay-hints-enabled t)
```

### UI Enhancements

```elisp
;; Enable breadcrumb
(setq lsp-headerline-breadcrumb-enable t)

;; Enable lens (code lens)
(setq lsp-lens-enable t)

;; UI documentation
(setq lsp-ui-doc-enable t
      lsp-ui-doc-position 'at-point
      lsp-ui-doc-delay 0.5)

;; Sideline (diagnostics, code actions)
(setq lsp-ui-sideline-enable t
      lsp-ui-sideline-show-diagnostics t
      lsp-ui-sideline-show-code-actions t)
```

## Troubleshooting

### ALS Not Starting

1. Check ALS path is correct and absolute
2. Test ALS manually: `/path/to/als`
3. Check `*lsp-log*` buffer for errors
4. Enable debug: `(setq lsp-log-io t)`

### No Completions

1. Ensure adesh-mode is active: `M-x describe-mode`
2. Check LSP is connected: `M-x lsp-describe-session`
3. Verify company-mode is active: `M-x company-mode`

### Performance Issues

```elisp
;; Increase read size
(setq read-process-output-max (* 1024 1024))  ; 1MB

;; Disable file watchers
(setq lsp-enable-file-watchers nil)
```

## License

MIT License - see main repository LICENSE file.
