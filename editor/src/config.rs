use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineNumberMode {
    Absolute,
    Relative,
    None,
}

impl Default for LineNumberMode {
    fn default() -> Self {
        LineNumberMode::Absolute
    }
}

impl LineNumberMode {
    pub fn name(&self) -> &'static str {
        match self {
            LineNumberMode::Absolute => "absolute",
            LineNumberMode::Relative => "relative",
            LineNumberMode::None => "none",
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            LineNumberMode::Absolute => LineNumberMode::Relative,
            LineNumberMode::Relative => LineNumberMode::None,
            LineNumberMode::None => LineNumberMode::Absolute,
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "relative" | "rel" => LineNumberMode::Relative,
            "none" | "off" => LineNumberMode::None,
            _ => LineNumberMode::Absolute,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_tab_width")]
    pub tab_width: usize,
    #[serde(default = "default_use_spaces")]
    pub use_spaces: bool,
    #[serde(default = "default_line_numbers")]
    pub line_numbers: String,
    #[serde(default = "default_backend")]
    pub default_backend: String,
    #[serde(default)]
    pub language_server_path: Option<String>,
    #[serde(default = "default_true")]
    pub auto_save: bool,
    #[serde(default = "default_true")]
    pub word_wrap: bool,
    #[serde(default = "default_true")]
    pub show_line_highlight: bool,
    #[serde(default = "default_true")]
    pub auto_close_brackets: bool,
    #[serde(default = "default_true")]
    pub show_trailing_whitespace: bool,
    #[serde(default = "default_true")]
    pub rainbow_brackets: bool,
    #[serde(default = "default_true")]
    pub indent_guides: bool,
}

fn default_theme() -> String {
    "Adesh Dark".to_string()
}

fn default_tab_width() -> usize {
    4
}

fn default_use_spaces() -> bool {
    true
}

fn default_line_numbers() -> String {
    "absolute".to_string()
}

fn default_backend() -> String {
    "Interpreter".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            tab_width: default_tab_width(),
            use_spaces: default_use_spaces(),
            line_numbers: default_line_numbers(),
            default_backend: default_backend(),
            language_server_path: None,
            auto_save: false,
            word_wrap: false,
            show_line_highlight: true,
            auto_close_brackets: true,
            show_trailing_whitespace: true,
            rainbow_brackets: true,
            indent_guides: true,
        }
    }
}

impl EditorConfig {
    pub fn load() -> Self {
        if let Some(config_path) = Self::config_file_path() {
            if config_path.is_file() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(cfg) = toml::from_str(&content) {
                        return cfg;
                    }
                }
            }
        }
        Self::default()
    }

    pub fn config_file_path() -> Option<PathBuf> {
        if let Some(config_dir) = dirs::config_dir() {
            Some(config_dir.join("adesh").join("editor.toml"))
        } else if let Some(home) = dirs::home_dir() {
            Some(home.join(".config").join("adesh").join("editor.toml"))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub bg: Color,
    pub fg: Color,
    pub keyword: Color,
    pub string: Color,
    pub number: Color,
    pub comment: Color,
    pub type_color: Color,
    pub function: Color,
    pub operator: Color,
    pub line_number: Color,
    pub line_number_curr: Color,
    pub status_bg: Color,
    pub status_fg: Color,
    pub selection_bg: Color,
    pub error: Color,
    pub warning: Color,
    pub info: Color,
    pub hint: Color,
    pub current_line_bg: Color,
    pub matching_bracket: Color,
    pub trailing_ws: Color,
    pub indent_guide: Color,
    pub rainbow_brackets: [Color; 6],
    pub popup_bg: Color,
    pub popup_border: Color,
    pub breadcrumb_bg: Color,
}

impl Theme {
    pub fn adesh_dark() -> Self {
        Self {
            name: "Adesh Dark".to_string(),
            bg: Color::Reset,
            fg: Color::Rgb(220, 220, 220),
            keyword: Color::Rgb(198, 120, 221),
            string: Color::Rgb(152, 195, 121),
            number: Color::Rgb(209, 154, 102),
            comment: Color::Rgb(92, 99, 112),
            type_color: Color::Rgb(229, 192, 123),
            function: Color::Rgb(97, 175, 239),
            operator: Color::Rgb(86, 182, 194),
            line_number: Color::Rgb(92, 99, 112),
            line_number_curr: Color::Rgb(229, 192, 123),
            status_bg: Color::Rgb(33, 37, 43),
            status_fg: Color::Rgb(171, 178, 191),
            selection_bg: Color::Rgb(62, 68, 81),
            error: Color::Rgb(224, 108, 117),
            warning: Color::Rgb(229, 192, 123),
            info: Color::Rgb(97, 175, 239),
            hint: Color::Rgb(86, 182, 194),
            current_line_bg: Color::Rgb(28, 32, 38),
            matching_bracket: Color::Rgb(86, 182, 194),
            trailing_ws: Color::Rgb(224, 108, 117),
            indent_guide: Color::Rgb(50, 54, 62),
            rainbow_brackets: [
                Color::Rgb(229, 192, 123), // Gold
                Color::Rgb(198, 120, 221), // Purple
                Color::Rgb(97, 175, 239),  // Blue
                Color::Rgb(152, 195, 121), // Green
                Color::Rgb(86, 182, 194),  // Cyan
                Color::Rgb(224, 108, 117), // Red
            ],
            popup_bg: Color::Rgb(30, 34, 42),
            popup_border: Color::Rgb(97, 175, 239),
            breadcrumb_bg: Color::Rgb(25, 28, 34),
        }
    }

    pub fn tokyo_night() -> Self {
        Self {
            name: "Tokyo Night".to_string(),
            bg: Color::Reset,
            fg: Color::Rgb(192, 202, 245),
            keyword: Color::Rgb(187, 154, 247),
            string: Color::Rgb(158, 206, 106),
            number: Color::Rgb(255, 158, 100),
            comment: Color::Rgb(86, 95, 137),
            type_color: Color::Rgb(42, 195, 222),
            function: Color::Rgb(122, 162, 247),
            operator: Color::Rgb(137, 221, 255),
            line_number: Color::Rgb(54, 60, 86),
            line_number_curr: Color::Rgb(255, 158, 100),
            status_bg: Color::Rgb(22, 22, 30),
            status_fg: Color::Rgb(169, 177, 214),
            selection_bg: Color::Rgb(54, 62, 94),
            error: Color::Rgb(247, 118, 142),
            warning: Color::Rgb(224, 175, 104),
            info: Color::Rgb(122, 162, 247),
            hint: Color::Rgb(16, 185, 129),
            current_line_bg: Color::Rgb(31, 35, 53),
            matching_bracket: Color::Rgb(187, 154, 247),
            trailing_ws: Color::Rgb(247, 118, 142),
            indent_guide: Color::Rgb(41, 46, 66),
            rainbow_brackets: [
                Color::Rgb(255, 158, 100),
                Color::Rgb(187, 154, 247),
                Color::Rgb(122, 162, 247),
                Color::Rgb(158, 206, 106),
                Color::Rgb(42, 195, 222),
                Color::Rgb(247, 118, 142),
            ],
            popup_bg: Color::Rgb(22, 22, 30),
            popup_border: Color::Rgb(122, 162, 247),
            breadcrumb_bg: Color::Rgb(19, 20, 27),
        }
    }

    pub fn catppuccin_mocha() -> Self {
        Self {
            name: "Catppuccin Mocha".to_string(),
            bg: Color::Reset,
            fg: Color::Rgb(205, 214, 244),
            keyword: Color::Rgb(203, 166, 247),
            string: Color::Rgb(166, 227, 161),
            number: Color::Rgb(250, 179, 135),
            comment: Color::Rgb(108, 112, 134),
            type_color: Color::Rgb(249, 226, 175),
            function: Color::Rgb(137, 180, 250),
            operator: Color::Rgb(148, 226, 213),
            line_number: Color::Rgb(88, 91, 112),
            line_number_curr: Color::Rgb(203, 166, 247),
            status_bg: Color::Rgb(24, 24, 37),
            status_fg: Color::Rgb(186, 194, 222),
            selection_bg: Color::Rgb(69, 71, 90),
            error: Color::Rgb(243, 139, 168),
            warning: Color::Rgb(249, 226, 175),
            info: Color::Rgb(137, 180, 250),
            hint: Color::Rgb(148, 226, 213),
            current_line_bg: Color::Rgb(30, 30, 46),
            matching_bracket: Color::Rgb(245, 194, 231),
            trailing_ws: Color::Rgb(243, 139, 168),
            indent_guide: Color::Rgb(49, 50, 68),
            rainbow_brackets: [
                Color::Rgb(249, 226, 175),
                Color::Rgb(203, 166, 247),
                Color::Rgb(137, 180, 250),
                Color::Rgb(166, 227, 161),
                Color::Rgb(148, 226, 213),
                Color::Rgb(243, 139, 168),
            ],
            popup_bg: Color::Rgb(24, 24, 37),
            popup_border: Color::Rgb(203, 166, 247),
            breadcrumb_bg: Color::Rgb(17, 17, 27),
        }
    }

    pub fn nord() -> Self {
        Self {
            name: "Nord".to_string(),
            bg: Color::Reset,
            fg: Color::Rgb(236, 239, 244),
            keyword: Color::Rgb(129, 161, 193),
            string: Color::Rgb(163, 190, 140),
            number: Color::Rgb(180, 142, 173),
            comment: Color::Rgb(76, 86, 106),
            type_color: Color::Rgb(143, 188, 187),
            function: Color::Rgb(136, 192, 208),
            operator: Color::Rgb(129, 161, 193),
            line_number: Color::Rgb(76, 86, 106),
            line_number_curr: Color::Rgb(136, 192, 208),
            status_bg: Color::Rgb(46, 52, 64),
            status_fg: Color::Rgb(216, 222, 233),
            selection_bg: Color::Rgb(67, 76, 94),
            error: Color::Rgb(191, 97, 106),
            warning: Color::Rgb(235, 203, 139),
            info: Color::Rgb(136, 192, 208),
            hint: Color::Rgb(143, 188, 187),
            current_line_bg: Color::Rgb(59, 66, 82),
            matching_bracket: Color::Rgb(136, 192, 208),
            trailing_ws: Color::Rgb(191, 97, 106),
            indent_guide: Color::Rgb(59, 66, 82),
            rainbow_brackets: [
                Color::Rgb(235, 203, 139),
                Color::Rgb(180, 142, 173),
                Color::Rgb(136, 192, 208),
                Color::Rgb(163, 190, 140),
                Color::Rgb(143, 188, 187),
                Color::Rgb(191, 97, 106),
            ],
            popup_bg: Color::Rgb(46, 52, 64),
            popup_border: Color::Rgb(136, 192, 208),
            breadcrumb_bg: Color::Rgb(36, 41, 51),
        }
    }

    pub fn dracula() -> Self {
        Self {
            name: "Dracula".to_string(),
            bg: Color::Reset,
            fg: Color::Rgb(248, 248, 242),
            keyword: Color::Rgb(255, 121, 198),
            string: Color::Rgb(241, 250, 140),
            number: Color::Rgb(189, 147, 249),
            comment: Color::Rgb(98, 114, 164),
            type_color: Color::Rgb(139, 233, 253),
            function: Color::Rgb(80, 250, 123),
            operator: Color::Rgb(255, 184, 108),
            line_number: Color::Rgb(98, 114, 164),
            line_number_curr: Color::Rgb(255, 121, 198),
            status_bg: Color::Rgb(33, 34, 44),
            status_fg: Color::Rgb(248, 248, 242),
            selection_bg: Color::Rgb(68, 71, 90),
            error: Color::Rgb(255, 85, 85),
            warning: Color::Rgb(241, 250, 140),
            info: Color::Rgb(139, 233, 253),
            hint: Color::Rgb(80, 250, 123),
            current_line_bg: Color::Rgb(40, 42, 54),
            matching_bracket: Color::Rgb(139, 233, 253),
            trailing_ws: Color::Rgb(255, 85, 85),
            indent_guide: Color::Rgb(68, 71, 90),
            rainbow_brackets: [
                Color::Rgb(255, 184, 108),
                Color::Rgb(255, 121, 198),
                Color::Rgb(139, 233, 253),
                Color::Rgb(80, 250, 123),
                Color::Rgb(189, 147, 249),
                Color::Rgb(255, 85, 85),
            ],
            popup_bg: Color::Rgb(40, 42, 54),
            popup_border: Color::Rgb(189, 147, 249),
            breadcrumb_bg: Color::Rgb(25, 26, 33),
        }
    }

    pub fn gruvbox() -> Self {
        Self {
            name: "Gruvbox".to_string(),
            bg: Color::Reset,
            fg: Color::Rgb(235, 219, 178),
            keyword: Color::Rgb(177, 98, 134),
            string: Color::Rgb(152, 151, 26),
            number: Color::Rgb(214, 93, 14),
            comment: Color::Rgb(146, 131, 116),
            type_color: Color::Rgb(250, 189, 47),
            function: Color::Rgb(184, 187, 38),
            operator: Color::Rgb(104, 157, 106),
            line_number: Color::Rgb(124, 111, 100),
            line_number_curr: Color::Rgb(250, 189, 47),
            status_bg: Color::Rgb(40, 40, 40),
            status_fg: Color::Rgb(235, 219, 178),
            selection_bg: Color::Rgb(90, 74, 54),
            error: Color::Rgb(204, 36, 29),
            warning: Color::Rgb(250, 189, 47),
            info: Color::Rgb(131, 165, 152),
            hint: Color::Rgb(104, 157, 106),
            current_line_bg: Color::Rgb(50, 48, 47),
            matching_bracket: Color::Rgb(104, 157, 106),
            trailing_ws: Color::Rgb(204, 36, 29),
            indent_guide: Color::Rgb(60, 56, 54),
            rainbow_brackets: [
                Color::Rgb(250, 189, 47),
                Color::Rgb(177, 98, 134),
                Color::Rgb(131, 165, 152),
                Color::Rgb(184, 187, 38),
                Color::Rgb(104, 157, 106),
                Color::Rgb(214, 93, 14),
            ],
            popup_bg: Color::Rgb(40, 40, 40),
            popup_border: Color::Rgb(250, 189, 47),
            breadcrumb_bg: Color::Rgb(29, 32, 33),
        }
    }

    pub fn monokai() -> Self {
        Self {
            name: "Monokai".to_string(),
            bg: Color::Reset,
            fg: Color::Rgb(248, 248, 242),
            keyword: Color::Rgb(249, 38, 114),
            string: Color::Rgb(230, 219, 116),
            number: Color::Rgb(174, 129, 255),
            comment: Color::Rgb(117, 113, 94),
            type_color: Color::Rgb(102, 217, 239),
            function: Color::Rgb(166, 226, 46),
            operator: Color::Rgb(249, 38, 114),
            line_number: Color::Rgb(117, 113, 94),
            line_number_curr: Color::Rgb(249, 38, 114),
            status_bg: Color::Rgb(39, 40, 34),
            status_fg: Color::Rgb(248, 248, 242),
            selection_bg: Color::Rgb(73, 72, 62),
            error: Color::Rgb(249, 38, 114),
            warning: Color::Rgb(230, 219, 116),
            info: Color::Rgb(102, 217, 239),
            hint: Color::Rgb(166, 226, 46),
            current_line_bg: Color::Rgb(49, 50, 44),
            matching_bracket: Color::Rgb(102, 217, 239),
            trailing_ws: Color::Rgb(249, 38, 114),
            indent_guide: Color::Rgb(60, 60, 50),
            rainbow_brackets: [
                Color::Rgb(230, 219, 116),
                Color::Rgb(249, 38, 114),
                Color::Rgb(102, 217, 239),
                Color::Rgb(166, 226, 46),
                Color::Rgb(174, 129, 255),
                Color::Rgb(253, 151, 31),
            ],
            popup_bg: Color::Rgb(39, 40, 34),
            popup_border: Color::Rgb(249, 38, 114),
            breadcrumb_bg: Color::Rgb(28, 29, 24),
        }
    }

    pub fn one_dark() -> Self {
        Self {
            name: "One Dark".to_string(),
            bg: Color::Reset,
            fg: Color::Rgb(171, 178, 191),
            keyword: Color::Rgb(198, 120, 221),
            string: Color::Rgb(152, 195, 121),
            number: Color::Rgb(209, 154, 102),
            comment: Color::Rgb(92, 99, 112),
            type_color: Color::Rgb(229, 192, 123),
            function: Color::Rgb(97, 175, 239),
            operator: Color::Rgb(86, 182, 194),
            line_number: Color::Rgb(92, 99, 112),
            line_number_curr: Color::Rgb(229, 192, 123),
            status_bg: Color::Rgb(33, 37, 43),
            status_fg: Color::Rgb(171, 178, 191),
            selection_bg: Color::Rgb(62, 68, 81),
            error: Color::Rgb(224, 108, 117),
            warning: Color::Rgb(229, 192, 123),
            info: Color::Rgb(97, 175, 239),
            hint: Color::Rgb(86, 182, 194),
            current_line_bg: Color::Rgb(40, 44, 52),
            matching_bracket: Color::Rgb(86, 182, 194),
            trailing_ws: Color::Rgb(224, 108, 117),
            indent_guide: Color::Rgb(50, 54, 62),
            rainbow_brackets: [
                Color::Rgb(229, 192, 123),
                Color::Rgb(198, 120, 221),
                Color::Rgb(97, 175, 239),
                Color::Rgb(152, 195, 121),
                Color::Rgb(86, 182, 194),
                Color::Rgb(224, 108, 117),
            ],
            popup_bg: Color::Rgb(33, 37, 43),
            popup_border: Color::Rgb(97, 175, 239),
            breadcrumb_bg: Color::Rgb(24, 26, 31),
        }
    }

    pub fn rose_pine() -> Self {
        Self {
            name: "Rose Pine".to_string(),
            bg: Color::Reset,
            fg: Color::Rgb(224, 222, 244),
            keyword: Color::Rgb(156, 207, 216),
            string: Color::Rgb(235, 188, 186),
            number: Color::Rgb(246, 193, 119),
            comment: Color::Rgb(110, 106, 134),
            type_color: Color::Rgb(235, 111, 146),
            function: Color::Rgb(196, 167, 231),
            operator: Color::Rgb(49, 116, 143),
            line_number: Color::Rgb(110, 106, 134),
            line_number_curr: Color::Rgb(246, 193, 119),
            status_bg: Color::Rgb(31, 29, 46),
            status_fg: Color::Rgb(224, 222, 244),
            selection_bg: Color::Rgb(68, 65, 90),
            error: Color::Rgb(235, 111, 146),
            warning: Color::Rgb(246, 193, 119),
            info: Color::Rgb(156, 207, 216),
            hint: Color::Rgb(196, 167, 231),
            current_line_bg: Color::Rgb(38, 35, 58),
            matching_bracket: Color::Rgb(196, 167, 231),
            trailing_ws: Color::Rgb(235, 111, 146),
            indent_guide: Color::Rgb(50, 46, 70),
            rainbow_brackets: [
                Color::Rgb(246, 193, 119),
                Color::Rgb(196, 167, 231),
                Color::Rgb(156, 207, 216),
                Color::Rgb(235, 188, 186),
                Color::Rgb(49, 116, 143),
                Color::Rgb(235, 111, 146),
            ],
            popup_bg: Color::Rgb(31, 29, 46),
            popup_border: Color::Rgb(196, 167, 231),
            breadcrumb_bg: Color::Rgb(25, 23, 36),
        }
    }

    pub fn adesh_matrix() -> Self {
        Self {
            name: "Adesh Matrix".to_string(),
            bg: Color::Reset,
            fg: Color::Rgb(34, 197, 94),
            keyword: Color::Rgb(74, 222, 128),
            string: Color::Rgb(134, 239, 172),
            number: Color::Rgb(187, 247, 208),
            comment: Color::Rgb(22, 101, 52),
            type_color: Color::Rgb(163, 230, 53),
            function: Color::Rgb(52, 211, 153),
            operator: Color::Rgb(74, 222, 128),
            line_number: Color::Rgb(21, 128, 61),
            line_number_curr: Color::Rgb(134, 239, 172),
            status_bg: Color::Rgb(5, 46, 22),
            status_fg: Color::Rgb(134, 239, 172),
            selection_bg: Color::Rgb(20, 83, 45),
            error: Color::Rgb(239, 68, 68),
            warning: Color::Rgb(245, 158, 11),
            info: Color::Rgb(52, 211, 153),
            hint: Color::Rgb(74, 222, 128),
            current_line_bg: Color::Rgb(6, 78, 36),
            matching_bracket: Color::Rgb(187, 247, 208),
            trailing_ws: Color::Rgb(239, 68, 68),
            indent_guide: Color::Rgb(20, 83, 45),
            rainbow_brackets: [
                Color::Rgb(134, 239, 172),
                Color::Rgb(74, 222, 128),
                Color::Rgb(52, 211, 153),
                Color::Rgb(163, 230, 53),
                Color::Rgb(187, 247, 208),
                Color::Rgb(34, 197, 94),
            ],
            popup_bg: Color::Rgb(5, 46, 22),
            popup_border: Color::Rgb(74, 222, 128),
            breadcrumb_bg: Color::Rgb(2, 26, 12),
        }
    }

    pub fn adesh_light() -> Self {
        Self {
            name: "Adesh Light".to_string(),
            bg: Color::Rgb(250, 250, 250),
            fg: Color::Rgb(56, 58, 66),
            keyword: Color::Rgb(160, 22, 160),
            string: Color::Rgb(80, 161, 79),
            number: Color::Rgb(152, 104, 1),
            comment: Color::Rgb(160, 161, 167),
            type_color: Color::Rgb(193, 132, 1),
            function: Color::Rgb(64, 120, 242),
            operator: Color::Rgb(1, 132, 188),
            line_number: Color::Rgb(160, 161, 167),
            line_number_curr: Color::Rgb(64, 120, 242),
            status_bg: Color::Rgb(234, 234, 236),
            status_fg: Color::Rgb(56, 58, 66),
            selection_bg: Color::Rgb(215, 215, 219),
            error: Color::Rgb(228, 86, 73),
            warning: Color::Rgb(193, 132, 1),
            info: Color::Rgb(64, 120, 242),
            hint: Color::Rgb(1, 132, 188),
            current_line_bg: Color::Rgb(240, 240, 242),
            matching_bracket: Color::Rgb(1, 132, 188),
            trailing_ws: Color::Rgb(228, 86, 73),
            indent_guide: Color::Rgb(220, 220, 225),
            rainbow_brackets: [
                Color::Rgb(193, 132, 1),
                Color::Rgb(160, 22, 160),
                Color::Rgb(64, 120, 242),
                Color::Rgb(80, 161, 79),
                Color::Rgb(1, 132, 188),
                Color::Rgb(228, 86, 73),
            ],
            popup_bg: Color::Rgb(245, 245, 247),
            popup_border: Color::Rgb(64, 120, 242),
            breadcrumb_bg: Color::Rgb(230, 230, 233),
        }
    }

    pub fn all_themes() -> Vec<Theme> {
        vec![
            Self::adesh_dark(),
            Self::tokyo_night(),
            Self::catppuccin_mocha(),
            Self::nord(),
            Self::dracula(),
            Self::gruvbox(),
            Self::monokai(),
            Self::one_dark(),
            Self::rose_pine(),
            Self::adesh_matrix(),
            Self::adesh_light(),
        ]
    }

    pub fn get_by_name(name: &str) -> Self {
        for theme in Self::all_themes() {
            if theme.name.to_lowercase() == name.to_lowercase() {
                return theme;
            }
        }
        for theme in Self::all_themes() {
            if theme.name.to_lowercase().contains(&name.to_lowercase()) {
                return theme;
            }
        }
        Self::adesh_dark()
    }

    pub fn cycle(&self) -> Self {
        let themes = Self::all_themes();
        let idx = themes.iter().position(|t| t.name == self.name).unwrap_or(0);
        themes[(idx + 1) % themes.len()].clone()
    }
}
