//! Column Operations Dialog
//!
//! Dialog for configuring column-level operations like generating embeddings,
//! PCA dimensionality reduction, clustering, and prompt-similarity sorting.

use crate::core::llm_config::LlmProvider;
use crate::tui::{Action, Component, Focusable, KeyEventResult, Theme};
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

// ── Operation kind ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnOperationKind {
    GenerateEmbeddings,
    Pca,
    Cluster,
    SortByPromptSimilarity,
}

impl ColumnOperationKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::GenerateEmbeddings => "Generate Embeddings",
            Self::Pca => "PCA Reduction",
            Self::Cluster => "Cluster",
            Self::SortByPromptSimilarity => "Sort by Prompt Similarity",
        }
    }
}

// ── Supporting option types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterAlgorithm {
    Kmeans,
    Dbscan,
}

impl ClusterAlgorithm {
    fn label(self) -> &'static str {
        match self {
            Self::Kmeans => "KMeans",
            Self::Dbscan => "DBSCAN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct KmeansOptions {
    pub number_of_clusters: usize,
    pub runs: usize,
    pub tolerance: usize,
}

impl Default for KmeansOptions {
    fn default() -> Self {
        Self {
            number_of_clusters: 8,
            runs: 1,
            tolerance: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DbscanOptions {
    pub minimum_points: usize,
    pub tolerance: usize,
}

impl Default for DbscanOptions {
    fn default() -> Self {
        Self {
            minimum_points: 5,
            tolerance: 1,
        }
    }
}

// ── Result produced when the user applies ─────────────────────────────────────

#[derive(Debug, Clone)]
pub enum OperationOptions {
    GenerateEmbeddings {
        model_name: String,
        num_dimensions: usize,
    },
    Pca {
        target_embedding_size: usize,
    },
    Cluster {
        algorithm: ClusterAlgorithm,
        kmeans: Option<KmeansOptions>,
        dbscan: Option<DbscanOptions>,
    },
    SortByPromptSimilarity {
        prompt: String,
    },
}

#[derive(Debug, Clone)]
pub struct ColumnOperationConfig {
    pub operation: ColumnOperationKind,
    pub new_column_name: String,
    pub source_column: String,
    pub hide_new_column: bool,
    pub provider: LlmProvider,
    pub options: OperationOptions,
}

// ── Dialog result ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum DialogResult {
    Applied(ColumnOperationConfig),
    Cancelled,
}

// ── Main dialog struct ────────────────────────────────────────────────────────

pub struct ColumnOperationOptionsDialog {
    pub focused: bool,
    pub closed: bool,
    pub result: Option<DialogResult>,
    pub show_instructions: bool,

    // Operation being configured
    operation: ColumnOperationKind,

    // Common fields
    new_column_name: String,
    new_column_cursor: usize,
    hide_new_column: bool,

    // Column selection
    columns: Vec<String>,
    selected_column_index: usize,

    // Provider / embedding
    selected_provider: LlmProvider,
    model_name: String,
    model_name_cursor: usize,
    num_dimensions: usize,
    num_dimensions_str: String,

    // PCA
    target_embedding_size: usize,
    target_embedding_size_str: String,

    // Clustering
    cluster_algorithm: ClusterAlgorithm,
    kmeans: KmeansOptions,
    dbscan: DbscanOptions,
    kmeans_num_clusters_str: String,
    kmeans_runs_str: String,
    kmeans_tolerance_str: String,
    dbscan_min_points_str: String,
    dbscan_tolerance_str: String,
    prompt: String,
    prompt_cursor: usize,

    // Focus tracking
    selected_field: usize, // 0-based index into fields_for_operation()
    in_buttons: bool,
    selected_button: usize, // 0 = Apply, 1 = Cancel

    // Error message to show inline
    error: Option<String>,
}

impl ColumnOperationOptionsDialog {
    pub fn new(
        operation: ColumnOperationKind,
        columns: Vec<String>,
        selected_column_index: usize,
    ) -> Self {
        let selected_column_index = selected_column_index.min(columns.len().saturating_sub(1));
        Self {
            focused: true,
            closed: false,
            result: None,
            show_instructions: false,
            operation,
            new_column_name: String::new(),
            new_column_cursor: 0,
            hide_new_column: false,
            columns,
            selected_column_index,
            selected_provider: LlmProvider::OpenAI,
            model_name: "text-embedding-3-small".to_string(),
            model_name_cursor: "text-embedding-3-small".len(),
            num_dimensions: 1536,
            num_dimensions_str: "1536".to_string(),
            target_embedding_size: 0,
            target_embedding_size_str: "0".to_string(),
            cluster_algorithm: ClusterAlgorithm::Kmeans,
            kmeans: KmeansOptions::default(),
            dbscan: DbscanOptions::default(),
            kmeans_num_clusters_str: "8".to_string(),
            kmeans_runs_str: "1".to_string(),
            kmeans_tolerance_str: "1".to_string(),
            dbscan_min_points_str: "5".to_string(),
            dbscan_tolerance_str: "1".to_string(),
            prompt: String::new(),
            prompt_cursor: 0,
            selected_field: 0,
            in_buttons: false,
            selected_button: 0,
            error: None,
        }
    }

    // ── Field enumeration ─────────────────────────────────────────────────────

    /// Returns a list of (label, kind) pairs for the currently configured operation.
    /// kind is one of: "text", "number", "toggle", "enum"
    fn fields(&self) -> Vec<(&'static str, &'static str)> {
        let mut f = vec![("New Column Name", "text"), ("Source Column", "enum")];
        match self.operation {
            ColumnOperationKind::GenerateEmbeddings => {
                f.push(("Hide New Column", "toggle"));
                f.push(("Provider", "enum"));
                f.push(("Model Name", "text"));
                f.push(("Number of Dimensions", "number"));
            }
            ColumnOperationKind::Pca => {
                f.push(("Target Embedding Size", "number"));
            }
            ColumnOperationKind::Cluster => {
                f.push(("Algorithm", "enum"));
                match self.cluster_algorithm {
                    ClusterAlgorithm::Kmeans => {
                        f.push(("Number of Clusters", "number"));
                        f.push(("Runs", "number"));
                        f.push(("Tolerance", "number"));
                    }
                    ClusterAlgorithm::Dbscan => {
                        f.push(("Minimum Points", "number"));
                        f.push(("Tolerance", "number"));
                    }
                }
            }
            ColumnOperationKind::SortByPromptSimilarity => {
                f.push(("Prompt", "text"));
                f.push(("Provider", "enum"));
                f.push(("Model Name", "text"));
            }
        }
        f
    }

    /// Current display value for a field at index `i`.
    fn field_value(&self, i: usize) -> String {
        match i {
            0 => self.new_column_name.clone(),
            1 => self
                .columns
                .get(self.selected_column_index)
                .cloned()
                .unwrap_or_default(),
            _ => match self.operation {
                ColumnOperationKind::GenerateEmbeddings => match i {
                    2 => {
                        if self.hide_new_column {
                            "On".into()
                        } else {
                            "Off".into()
                        }
                    }
                    3 => self.selected_provider.display_name().into(),
                    4 => self.model_name.clone(),
                    5 => self.num_dimensions_str.clone(),
                    _ => String::new(),
                },
                ColumnOperationKind::Pca => match i {
                    2 => self.target_embedding_size_str.clone(),
                    _ => String::new(),
                },
                ColumnOperationKind::Cluster => match i {
                    2 => self.cluster_algorithm.label().into(),
                    3 => match self.cluster_algorithm {
                        ClusterAlgorithm::Kmeans => self.kmeans_num_clusters_str.clone(),
                        ClusterAlgorithm::Dbscan => self.dbscan_min_points_str.clone(),
                    },
                    4 => match self.cluster_algorithm {
                        ClusterAlgorithm::Kmeans => self.kmeans_runs_str.clone(),
                        ClusterAlgorithm::Dbscan => self.dbscan_tolerance_str.clone(),
                    },
                    5 => self.kmeans_tolerance_str.clone(),
                    _ => String::new(),
                },
                ColumnOperationKind::SortByPromptSimilarity => match i {
                    2 => self.prompt.clone(),
                    3 => self.selected_provider.display_name().into(),
                    4 => self.model_name.clone(),
                    _ => String::new(),
                },
            },
        }
    }

    fn current_field_kind(&self) -> &'static str {
        let fields = self.fields();
        fields
            .get(self.selected_field)
            .map(|(_, k)| *k)
            .unwrap_or("text")
    }

    fn num_string_for_field_mut(&mut self) -> Option<(&mut String, &mut usize)> {
        match (self.operation, self.selected_field) {
            (ColumnOperationKind::GenerateEmbeddings, 5) => {
                Some((&mut self.num_dimensions_str, &mut self.num_dimensions))
            }
            (ColumnOperationKind::Pca, 2) => Some((
                &mut self.target_embedding_size_str,
                &mut self.target_embedding_size,
            )),
            (ColumnOperationKind::Cluster, 3) => match self.cluster_algorithm {
                ClusterAlgorithm::Kmeans => Some((
                    &mut self.kmeans_num_clusters_str,
                    &mut self.kmeans.number_of_clusters,
                )),
                ClusterAlgorithm::Dbscan => Some((
                    &mut self.dbscan_min_points_str,
                    &mut self.dbscan.minimum_points,
                )),
            },
            (ColumnOperationKind::Cluster, 4) => match self.cluster_algorithm {
                ClusterAlgorithm::Kmeans => {
                    Some((&mut self.kmeans_runs_str, &mut self.kmeans.runs))
                }
                ClusterAlgorithm::Dbscan => {
                    Some((&mut self.dbscan_tolerance_str, &mut self.dbscan.tolerance))
                }
            },
            (ColumnOperationKind::Cluster, 5) => {
                Some((&mut self.kmeans_tolerance_str, &mut self.kmeans.tolerance))
            }
            _ => None,
        }
    }

    // ── Text input helpers ────────────────────────────────────────────────────

    fn text_field_mut(&mut self) -> Option<(&mut String, &mut usize)> {
        match self.selected_field {
            0 => Some((&mut self.new_column_name, &mut self.new_column_cursor)),
            4 if self.operation == ColumnOperationKind::GenerateEmbeddings
                || self.operation == ColumnOperationKind::SortByPromptSimilarity =>
            {
                Some((&mut self.model_name, &mut self.model_name_cursor))
            }
            2 if self.operation == ColumnOperationKind::SortByPromptSimilarity => {
                Some((&mut self.prompt, &mut self.prompt_cursor))
            }
            _ => None,
        }
    }

    fn insert_char(&mut self, ch: char) {
        if let Some((s, cursor)) = self.text_field_mut() {
            if *cursor <= s.len() {
                s.insert(*cursor, ch);
                *cursor += 1;
            }
        } else if self.current_field_kind() == "number" {
            if ch.is_ascii_digit() {
                if let Some((ns, nv)) = self.num_string_for_field_mut() {
                    if *ns == "0" {
                        *ns = ch.to_string();
                    } else {
                        ns.push(ch);
                    }
                    if let Ok(v) = ns.parse::<usize>() {
                        *nv = v;
                    }
                }
            }
        }
    }

    fn backspace(&mut self) {
        if let Some((s, cursor)) = self.text_field_mut() {
            if *cursor > 0 && !s.is_empty() {
                s.remove(*cursor - 1);
                *cursor -= 1;
            }
        } else if self.current_field_kind() == "number" {
            if let Some((ns, nv)) = self.num_string_for_field_mut() {
                if !ns.is_empty() {
                    ns.pop();
                    if ns.is_empty() {
                        *ns = "0".to_string();
                    }
                    if let Ok(v) = ns.parse::<usize>() {
                        *nv = v;
                    }
                }
            }
        }
    }

    fn cursor_left(&mut self) {
        if let Some((_, cursor)) = self.text_field_mut() {
            if *cursor > 0 {
                *cursor -= 1;
            }
        }
    }

    fn cursor_right(&mut self) {
        // borrow sep because text_field_mut borrows self mutably
        let len = match self.selected_field {
            0 => self.new_column_name.len(),
            4 if self.operation == ColumnOperationKind::GenerateEmbeddings
                || self.operation == ColumnOperationKind::SortByPromptSimilarity =>
            {
                self.model_name.len()
            }
            2 if self.operation == ColumnOperationKind::SortByPromptSimilarity => self.prompt.len(),
            _ => return,
        };
        let cursor = match self.selected_field {
            0 => &mut self.new_column_cursor,
            4 => &mut self.model_name_cursor,
            _ => return,
        };
        if *cursor < len {
            *cursor += 1;
        }
    }

    // ── Enum / toggle field cycling ───────────────────────────────────────────

    fn cycle_enum_field(&mut self, forward: bool) {
        match self.selected_field {
            1 => {
                // source column
                let n = self.columns.len();
                if n == 0 {
                    return;
                }
                if forward {
                    self.selected_column_index = (self.selected_column_index + 1) % n;
                } else {
                    self.selected_column_index = (self.selected_column_index + n - 1) % n;
                }
            }
            2 if self.operation == ColumnOperationKind::GenerateEmbeddings => {
                self.hide_new_column = !self.hide_new_column;
            }
            3 if self.operation == ColumnOperationKind::GenerateEmbeddings
                || self.operation == ColumnOperationKind::SortByPromptSimilarity =>
            {
                let providers = [LlmProvider::OpenAI, LlmProvider::Azure, LlmProvider::Ollama];
                let idx = providers
                    .iter()
                    .position(|p| *p == self.selected_provider)
                    .unwrap_or(0);
                let next = if forward {
                    (idx + 1) % providers.len()
                } else {
                    (idx + providers.len() - 1) % providers.len()
                };
                self.selected_provider = providers[next];
                // auto-update model to first for provider
                let models = Self::models_for_provider(self.selected_provider);
                if let Some((m, d)) = models.first() {
                    self.model_name = m.to_string();
                    self.model_name_cursor = self.model_name.len();
                    if self.operation == ColumnOperationKind::GenerateEmbeddings {
                        self.num_dimensions = *d;
                        self.num_dimensions_str = d.to_string();
                    }
                }
            }
            2 if self.operation == ColumnOperationKind::Cluster => {
                self.cluster_algorithm = match self.cluster_algorithm {
                    ClusterAlgorithm::Kmeans => ClusterAlgorithm::Dbscan,
                    ClusterAlgorithm::Dbscan => ClusterAlgorithm::Kmeans,
                };
                // reset field selection if it is now out-of-range
                let max = self.fields().len();
                if self.selected_field >= max {
                    self.selected_field = max.saturating_sub(1);
                }
            }
            _ => {}
        }
    }

    fn cycle_model(&mut self, forward: bool) {
        let models = Self::models_for_provider(self.selected_provider);
        if models.is_empty() {
            return;
        }
        let idx = models
            .iter()
            .position(|(m, _)| *m == self.model_name.as_str())
            .unwrap_or(0);
        let next = if forward {
            (idx + 1) % models.len()
        } else {
            (idx + models.len() - 1) % models.len()
        };
        let (m, d) = models[next];
        self.model_name = m.to_string();
        self.model_name_cursor = self.model_name.len();
        self.num_dimensions = d;
        self.num_dimensions_str = d.to_string();
    }

    fn models_for_provider(provider: LlmProvider) -> Vec<(&'static str, usize)> {
        match provider {
            LlmProvider::OpenAI | LlmProvider::Azure => vec![
                ("text-embedding-3-small", 1536),
                ("text-embedding-3-large", 3072),
            ],
            LlmProvider::Ollama => vec![("nomic-embed-text", 768), ("mxbai-embed-large", 1024)],
        }
    }

    // ── Apply ─────────────────────────────────────────────────────────────────

    fn try_apply(&mut self) {
        let source_column = self
            .columns
            .get(self.selected_column_index)
            .cloned()
            .unwrap_or_default();

        let options = match self.operation {
            ColumnOperationKind::GenerateEmbeddings => {
                if self.model_name.is_empty() {
                    self.error = Some("Model name cannot be empty.".into());
                    return;
                }
                OperationOptions::GenerateEmbeddings {
                    model_name: self.model_name.clone(),
                    num_dimensions: self.num_dimensions,
                }
            }
            ColumnOperationKind::Pca => {
                if self.target_embedding_size == 0 {
                    self.error = Some("Target embedding size must be > 0.".into());
                    return;
                }
                OperationOptions::Pca {
                    target_embedding_size: self.target_embedding_size,
                }
            }
            ColumnOperationKind::Cluster => OperationOptions::Cluster {
                algorithm: self.cluster_algorithm,
                kmeans: if matches!(self.cluster_algorithm, ClusterAlgorithm::Kmeans) {
                    Some(self.kmeans.clone())
                } else {
                    None
                },
                dbscan: if matches!(self.cluster_algorithm, ClusterAlgorithm::Dbscan) {
                    Some(self.dbscan.clone())
                } else {
                    None
                },
            },
            ColumnOperationKind::SortByPromptSimilarity => {
                if self.prompt.trim().is_empty() {
                    self.error = Some("Prompt cannot be empty.".into());
                    return;
                }
                OperationOptions::SortByPromptSimilarity {
                    prompt: self.prompt.clone(),
                }
            }
        };

        self.result = Some(DialogResult::Applied(ColumnOperationConfig {
            operation: self.operation,
            new_column_name: self.new_column_name.clone(),
            source_column,
            hide_new_column: self.hide_new_column,
            provider: self.selected_provider,
            options,
        }));
        self.closed = true;
    }

    // ── Retrieval ─────────────────────────────────────────────────────────────

    pub fn take_result(&mut self) -> Option<DialogResult> {
        self.result.take()
    }
}

// ── Component trait ───────────────────────────────────────────────────────────

impl Component for ColumnOperationOptionsDialog {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<KeyEventResult> {
        if !self.focused {
            return Ok(KeyEventResult::Ignored);
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            // ── Escape / Cancel ───────────────────────────────────────────────
            KeyCode::Esc => {
                self.result = Some(DialogResult::Cancelled);
                self.closed = true;
                return Ok(KeyEventResult::Consumed);
            }

            // ── Enter / Ctrl+Enter ─────────────────────────────────────────────
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+Enter: apply from anywhere
                    self.try_apply();
                } else if self.in_buttons {
                    if self.selected_button == 0 {
                        self.try_apply();
                    } else {
                        self.result = Some(DialogResult::Cancelled);
                        self.closed = true;
                    }
                } else {
                    // Enter outside buttons → advance to next field, or to buttons
                    let max = self.fields().len();
                    if self.selected_field + 1 >= max {
                        self.in_buttons = true;
                        self.selected_button = 0;
                    } else {
                        self.selected_field += 1;
                    }
                }
                return Ok(KeyEventResult::Consumed);
            }

            // ── Navigation: Up ────────────────────────────────────────────────
            KeyCode::Up => {
                if self.in_buttons {
                    self.in_buttons = false;
                } else if self.selected_field > 0 {
                    self.selected_field -= 1;
                }
                return Ok(KeyEventResult::Consumed);
            }

            // ── Navigation: Down ─────────────────────────────────────────────
            KeyCode::Down => {
                let max = self.fields().len();
                if self.in_buttons {
                    // already at bottom
                } else if self.selected_field + 1 >= max {
                    self.in_buttons = true;
                    self.selected_button = 0;
                } else {
                    self.selected_field += 1;
                }
                return Ok(KeyEventResult::Consumed);
            }

            // ── Navigation: Tab → buttons ─────────────────────────────────────
            KeyCode::Tab => {
                if self.in_buttons {
                    self.selected_button = (self.selected_button + 1) % 2;
                } else {
                    // On model name field, Tab cycles models
                    if self.operation == ColumnOperationKind::GenerateEmbeddings
                        && self.selected_field == 4
                    {
                        self.cycle_model(true);
                    } else {
                        self.in_buttons = true;
                        self.selected_button = 0;
                    }
                }
                return Ok(KeyEventResult::Consumed);
            }

            KeyCode::BackTab => {
                if self.in_buttons {
                    self.in_buttons = false;
                } else if self.operation == ColumnOperationKind::GenerateEmbeddings
                    && self.selected_field == 4
                {
                    self.cycle_model(false);
                }
                return Ok(KeyEventResult::Consumed);
            }

            // ── Left / Right ──────────────────────────────────────────────────
            KeyCode::Left => {
                if self.in_buttons {
                    self.selected_button = self.selected_button.saturating_sub(1);
                } else {
                    let kind = self.current_field_kind();
                    if kind == "text" {
                        self.cursor_left();
                    } else if kind == "enum" || kind == "toggle" {
                        self.cycle_enum_field(false);
                    }
                }
                return Ok(KeyEventResult::Consumed);
            }

            KeyCode::Right => {
                if self.in_buttons {
                    self.selected_button = (self.selected_button + 1) % 2;
                } else {
                    let kind = self.current_field_kind();
                    if kind == "text" {
                        self.cursor_right();
                    } else if kind == "enum" || kind == "toggle" {
                        self.cycle_enum_field(true);
                    }
                }
                return Ok(KeyEventResult::Consumed);
            }

            // ── Space: toggle enum fields ──────────────────────────────────────
            KeyCode::Char(' ') => {
                let kind = self.current_field_kind();
                if kind == "toggle" || kind == "enum" {
                    self.cycle_enum_field(true);
                    return Ok(KeyEventResult::Consumed);
                }
            }

            // ── Ctrl+I: toggle instructions ───────────────────────────────────
            KeyCode::Char('i') | KeyCode::Char('I') if ctrl => {
                self.show_instructions = !self.show_instructions;
                return Ok(KeyEventResult::Consumed);
            }

            // ── Backspace ─────────────────────────────────────────────────────
            KeyCode::Backspace => {
                if !self.in_buttons {
                    self.backspace();
                }
                return Ok(KeyEventResult::Consumed);
            }

            // ── Printable characters ──────────────────────────────────────────
            KeyCode::Char(ch) if !ctrl => {
                if !self.in_buttons {
                    self.insert_char(ch);
                }
                return Ok(KeyEventResult::Consumed);
            }

            _ => {}
        }

        Ok(KeyEventResult::Ignored)
    }

    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::Cancel | Action::Escape => {
                self.result = Some(DialogResult::Cancelled);
                self.closed = true;
                Ok(false)
            }
            Action::Confirm => {
                if self.in_buttons && self.selected_button == 0 {
                    self.try_apply();
                } else if self.in_buttons {
                    self.result = Some(DialogResult::Cancelled);
                    self.closed = true;
                } else {
                    // Confirm outside buttons advances to next field, or applies at last field
                    let max = self.fields().len();
                    if self.selected_field + 1 >= max {
                        self.in_buttons = true;
                        self.selected_button = 0;
                    } else {
                        self.selected_field += 1;
                    }
                }
                Ok(true)
            }
            Action::MoveUp => {
                if self.in_buttons {
                    self.in_buttons = false;
                } else if self.selected_field > 0 {
                    self.selected_field -= 1;
                }
                Ok(true)
            }
            Action::MoveDown => {
                let max = self.fields().len();
                if !self.in_buttons && self.selected_field + 1 >= max {
                    self.in_buttons = true;
                    self.selected_button = 0;
                } else if !self.in_buttons {
                    self.selected_field += 1;
                }
                Ok(true)
            }
            Action::MoveLeft => {
                if self.in_buttons {
                    self.selected_button = self.selected_button.saturating_sub(1);
                } else {
                    let kind = self.current_field_kind();
                    if kind == "text" {
                        self.cursor_left();
                    } else if kind == "enum" || kind == "toggle" {
                        self.cycle_enum_field(false);
                    }
                }
                Ok(true)
            }
            Action::MoveRight => {
                if self.in_buttons {
                    self.selected_button = (self.selected_button + 1) % 2;
                } else {
                    let kind = self.current_field_kind();
                    if kind == "text" {
                        self.cursor_right();
                    } else if kind == "enum" || kind == "toggle" {
                        self.cycle_enum_field(true);
                    }
                }
                Ok(true)
            }
            Action::ToggleInstructions => {
                self.show_instructions = !self.show_instructions;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        frame.render_widget(Clear, area);

        let border_style = if self.focused {
            theme.focused_border_style()
        } else {
            theme.border_style()
        };

        let outer_block = Block::default()
            .title(format!(" {} ", self.operation.title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(border_style);

        let inner = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        // Split inner area: content + optional instructions
        let (content_area, instr_area) = if self.show_instructions {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(5)])
                .split(inner);
            (chunks[0], Some(chunks[1]))
        } else {
            (inner, None)
        };

        // Render fields
        self.render_fields(frame, content_area, theme);

        // Render instructions
        if let Some(ia) = instr_area {
            self.render_instructions(frame, ia, theme);
        }
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::Cancel,
            Action::Escape,
            Action::Confirm,
            Action::MoveUp,
            Action::MoveDown,
            Action::MoveLeft,
            Action::MoveRight,
            Action::ToggleInstructions,
        ]
    }

    fn name(&self) -> &str {
        "ColumnOperationOptionsDialog"
    }
}

impl ColumnOperationOptionsDialog {
    fn render_fields(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let fields = self.fields();
        let total_fields = fields.len();

        // Reserve bottom row for buttons; remaining rows for fields
        let field_rows = area.height.saturating_sub(1) as usize;

        // Error bar (takes first row if set)
        let (err_offset, draw_area) = if let Some(ref err) = self.error {
            let err_area = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 1,
            };
            let msg = format!(" ✗ {err}");
            frame.render_widget(
                Paragraph::new(msg).style(theme.error_style().add_modifier(Modifier::BOLD)),
                err_area,
            );
            (
                1u16,
                Rect {
                    x: area.x,
                    y: area.y + 1,
                    width: area.width,
                    height: area.height.saturating_sub(1),
                },
            )
        } else {
            (0, area)
        };

        let _ = err_offset;

        // Render each field row
        for (i, (label, kind)) in fields.iter().enumerate().take(field_rows) {
            let y = draw_area.y + i as u16;
            if y >= draw_area.bottom() {
                break;
            }
            let is_active = !self.in_buttons && i == self.selected_field;

            let label_style = if is_active {
                theme.selected_style().add_modifier(Modifier::BOLD)
            } else {
                theme.normal_style()
            };

            let value = self.field_value(i);

            match *kind {
                "text" => {
                    // Label + inline text input with cursor
                    let full_label = format!("{label}: ");
                    let label_w = full_label.len() as u16;
                    frame.render_widget(
                        Paragraph::new(full_label.clone()).style(label_style),
                        Rect {
                            x: draw_area.x + 1,
                            y,
                            width: label_w.min(draw_area.width.saturating_sub(2)),
                            height: 1,
                        },
                    );
                    // Render the text with cursor
                    let cursor = match i {
                        0 => self.new_column_cursor,
                        4 => self.model_name_cursor,
                        _ => 0,
                    };
                    let input_x = draw_area.x + 1 + label_w;
                    let input_w = draw_area.width.saturating_sub(label_w + 2);
                    if input_w > 0 {
                        let cursor = cursor.min(value.len());
                        let before = &value[..cursor];
                        let cursor_char = value[cursor..].chars().next().unwrap_or(' ').to_string();
                        let after: String = value[cursor..].chars().skip(1).collect();
                        let spans = if is_active {
                            vec![
                                Span::styled(before, theme.normal_style()),
                                Span::styled(cursor_char, theme.selected_cell_style()),
                                Span::styled(after, theme.normal_style()),
                            ]
                        } else {
                            vec![Span::styled(value.clone(), theme.normal_style())]
                        };
                        frame.render_widget(
                            Paragraph::new(Line::from(spans)),
                            Rect {
                                x: input_x,
                                y,
                                width: input_w,
                                height: 1,
                            },
                        );
                    }
                }
                _ => {
                    // Enum / toggle / number: display as "Label: [value]"
                    let indicator = if is_active { "▶ " } else { "  " };
                    let line = format!("{indicator}{label}: {value}");
                    frame.render_widget(
                        Paragraph::new(line).style(label_style),
                        Rect {
                            x: draw_area.x,
                            y,
                            width: draw_area.width,
                            height: 1,
                        },
                    );
                }
            }

            // Hint for enum/number fields when selected
            if is_active && (*kind == "enum" || *kind == "toggle") {
                let hint = Span::styled(" ←/→ to change", theme.warning_style());
                let hint_x = draw_area.x + draw_area.width.saturating_sub(16);
                frame.render_widget(
                    Paragraph::new(Line::from(vec![hint])),
                    Rect {
                        x: hint_x,
                        y,
                        width: 16,
                        height: 1,
                    },
                );
            }
        }

        // ── Buttons ───────────────────────────────────────────────────────────
        let button_y = draw_area.y + total_fields.min(field_rows) as u16;
        if button_y < draw_area.bottom() {
            let buttons = ["[Apply]", "[Cancel]"];
            let mut x = draw_area.x + draw_area.width.saturating_sub(18);
            for (idx, btn) in buttons.iter().enumerate() {
                let style = if self.in_buttons && self.selected_button == idx {
                    theme.selected_style().add_modifier(Modifier::BOLD)
                } else if idx == 0 {
                    theme.success_style()
                } else {
                    theme.normal_style()
                };
                frame.render_widget(
                    Paragraph::new(*btn).style(style),
                    Rect {
                        x,
                        y: button_y,
                        width: btn.len() as u16 + 1,
                        height: 1,
                    },
                );
                x += btn.len() as u16 + 2;
            }
        }
    }

    fn render_instructions(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(" Instructions (Ctrl+i to hide) ")
            .borders(Borders::TOP)
            .border_style(theme.border_style());
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let op_hint = match self.operation {
            ColumnOperationKind::GenerateEmbeddings => {
                "• Tab on Model: cycle models  • ←/→: change provider/toggle"
            }
            ColumnOperationKind::Pca => "• ←/→ or type digits for target size",
            ColumnOperationKind::Cluster => {
                "• Space/←/→ on Algorithm: toggle  • digits: set numeric fields"
            }
            ColumnOperationKind::SortByPromptSimilarity => "• ←/→: choose source column",
        };

        let text = format!(
            "• ↑/↓: navigate fields  • ←/→ or Space: change value  • Tab: go to buttons\n• Enter: confirm selected button  • Esc: cancel\n{op_hint}"
        );

        frame.render_widget(
            Paragraph::new(text)
                .style(theme.warning_style())
                .wrap(Wrap { trim: true }),
            inner,
        );
    }
}

impl Focusable for ColumnOperationOptionsDialog {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}
