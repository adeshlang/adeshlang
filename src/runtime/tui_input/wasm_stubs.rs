//! WebAssembly stub implementation for tui_input subsystem.

#[derive(Debug, Clone, Default)]
pub struct InputOptions {
    pub default: Option<String>,
    pub trim: bool,
    pub masked: bool,
    pub timeout_ms: Option<u64>,
    pub suggestions: Vec<String>,
    pub allowed: Option<Vec<String>>,
    pub disallowed: Option<Vec<String>>,
    pub min_len: Option<usize>,
    pub not_empty: bool,
    pub multiline: bool,
    pub end_seq: Option<String>,
    pub style_color: Option<(u8, u8, u8)>,
}

#[derive(Debug, Clone, Default)]
pub struct CheckboxConfig {
    pub default: Vec<String>,
    pub required: bool,
    pub limit: Option<usize>,
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RadioConfig {
    pub default: Option<String>,
    pub required: bool,
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SelectConfig {
    pub default: Option<String>,
    pub filterable: bool,
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FormFieldConfig {
    pub prompt: String,
    pub default: Option<String>,
    pub masked: bool,
    pub field_type: String,
    pub options: Vec<String>,
}

impl Default for FormFieldConfig {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            default: None,
            masked: false,
            field_type: "text".into(),
            options: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FormConfig {
    pub title: Option<String>,
    pub prefix: String,
    pub suffix: String,
    pub separator: String,
}

#[derive(Debug, Clone, Default)]
pub struct PasswordConfig {
    pub mask_char: char,
    pub show_strength: bool,
    pub allow_peek: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FuzzyConfig {
    pub case_sensitive: bool,
    pub limit: Option<usize>,
    pub preview: bool,
}

#[derive(Debug, Clone)]
pub struct SliderConfig {
    pub unit: String,
    pub show_gauge: bool,
}

impl Default for SliderConfig {
    fn default() -> Self {
        Self {
            unit: String::new(),
            show_gauge: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub children: Vec<TreeNode>,
    pub expanded: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TreeConfig {
    pub allow_branch_selection: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TableConfig {
    pub editable: bool,
    pub sortable: bool,
    pub select_row: bool,
}

#[derive(Debug, Clone)]
pub struct TableResult {
    pub selected_row: usize,
    pub selected_col: usize,
    pub data: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct DatepickerConfig {
    pub min_year: Option<i32>,
    pub max_year: Option<i32>,
    pub default_year: Option<i32>,
    pub default_month: Option<u32>,
    pub default_day: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct DatetimeConfig {
    pub default_year: Option<i32>,
    pub default_month: Option<u32>,
    pub default_day: Option<u32>,
    pub default_hour: Option<u32>,
    pub default_minute: Option<u32>,
    pub format_24h: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TimepickerConfig {
    pub format_24h: bool,
    pub include_seconds: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ColorConfig {
    pub default_hex: Option<String>,
    pub format: String,
}

#[derive(Debug, Clone, Default)]
pub struct PinConfig {
    pub mask: bool,
    pub allow_alpha: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DiffConfig {
    pub context_lines: usize,
}

#[derive(Debug, Clone, Default)]
pub struct HotkeyConfig {
    pub instructions: String,
}

#[derive(Debug, Clone)]
pub struct HotkeyResult {
    pub key: String,
    pub modifiers: Vec<String>,
    pub chord: String,
}

#[derive(Debug, Clone, Default)]
pub struct AiPredictConfig {
    pub max_suggestions: usize,
}

#[derive(Debug, Clone, Default)]
pub struct StreamConfig {
    pub delimiter: Option<char>,
    pub max_items: Option<usize>,
}

pub struct TerminalGuard;

impl TerminalGuard {
    pub fn new_inline(_height: u16) -> std::io::Result<Self> {
        Ok(Self)
    }
    pub fn new_alternate() -> std::io::Result<Self> {
        Ok(Self)
    }
}

pub fn get_mock_input() -> Option<String> {
    None
}

pub fn read_line_prompt(_prompt: &str) -> String {
    String::new()
}

pub fn prompt_input(_prompt: &str, opts: &InputOptions) -> Result<String, String> {
    Ok(opts.default.clone().unwrap_or_default())
}

pub fn prompt_checkbox(_prompt: &str, _options: &[String], config: &CheckboxConfig) -> Result<Vec<String>, String> {
    Ok(config.default.clone())
}

pub fn prompt_radio(_prompt: &str, options: &[String], config: &RadioConfig) -> Result<String, String> {
    Ok(config.default.clone().unwrap_or_else(|| options.first().cloned().unwrap_or_default()))
}

pub fn prompt_select(_prompt: &str, options: &[String], config: &SelectConfig) -> Result<String, String> {
    Ok(config.default.clone().unwrap_or_else(|| options.first().cloned().unwrap_or_default()))
}

pub fn prompt_form(fields: &[(String, FormFieldConfig)], _config: &FormConfig) -> Result<Vec<(String, String)>, String> {
    Ok(fields.iter().map(|(name, f)| (name.clone(), f.default.clone().unwrap_or_default())).collect())
}

pub fn prompt_confirm(_prompt: &str, default: bool) -> Result<bool, String> {
    Ok(default)
}

pub fn prompt_password(_prompt: &str, _config: &PasswordConfig) -> Result<String, String> {
    Ok(String::new())
}

pub fn prompt_fuzzy(_prompt: &str, options: &[String], _config: &FuzzyConfig) -> Result<String, String> {
    Ok(options.first().cloned().unwrap_or_default())
}

pub fn prompt_slider(
    _prompt: &str,
    _min: f64,
    _max: f64,
    _step: f64,
    default: f64,
    _config: &SliderConfig,
) -> Result<f64, String> {
    Ok(default)
}

pub fn prompt_tree(_prompt: &str, nodes: &[TreeNode], _config: &TreeConfig) -> Result<String, String> {
    Ok(nodes.first().map(|n| n.id.clone()).unwrap_or_default())
}

pub fn prompt_table(
    _prompt: &str,
    _headers: &[String],
    rows: &[Vec<String>],
    _config: &TableConfig,
) -> Result<TableResult, String> {
    Ok(TableResult {
        selected_row: 0,
        selected_col: 0,
        data: rows.to_vec(),
    })
}

pub fn prompt_datepicker(_prompt: &str, config: &DatepickerConfig) -> Result<String, String> {
    let year = config.default_year.unwrap_or(2026);
    let month = config.default_month.unwrap_or(9);
    let day = config.default_day.unwrap_or(8);
    Ok(format!("{:04}-{:02}-{:02}", year, month, day))
}

pub fn prompt_datetimepicker(_prompt: &str, config: &DatetimeConfig) -> Result<String, String> {
    let year = config.default_year.unwrap_or(2026);
    let month = config.default_month.unwrap_or(9);
    let day = config.default_day.unwrap_or(8);
    let hour = config.default_hour.unwrap_or(12);
    let min = config.default_minute.unwrap_or(0);
    Ok(format!("{:04}-{:02}-{:02} {:02}:{:02}", year, month, day, hour, min))
}

pub fn prompt_timepicker(_prompt: &str, _config: &TimepickerConfig) -> Result<String, String> {
    Ok("12:00".to_string())
}

pub fn prompt_color(_prompt: &str, config: &ColorConfig) -> Result<String, String> {
    Ok(config.default_hex.clone().unwrap_or_else(|| "#000000".to_string()))
}

pub fn prompt_pin(_prompt: &str, digits: usize, _config: &PinConfig) -> Result<String, String> {
    Ok("0".repeat(digits))
}

pub fn prompt_diff(_prompt: &str, original: &str, _config: &DiffConfig) -> Result<String, String> {
    Ok(original.to_string())
}

pub fn prompt_hotkey(_prompt: &str, _config: &HotkeyConfig) -> Result<HotkeyResult, String> {
    Ok(HotkeyResult {
        key: "Enter".to_string(),
        modifiers: Vec::new(),
        chord: "Enter".to_string(),
    })
}

pub fn prompt_ai(_prompt: &str, _context: &[String], _config: &AiPredictConfig) -> Result<String, String> {
    Ok(String::new())
}

pub fn prompt_stream(_prompt: &str, _config: &StreamConfig) -> Result<Vec<String>, String> {
    Ok(Vec::new())
}
