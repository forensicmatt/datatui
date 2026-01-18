use crate::tui::action::Action;
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Maps Scopes to KeyPatterns to Actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindings {
    /// nested map: scope -> { key_string -> Action }
    pub scopes: HashMap<String, HashMap<String, Action>>,

    #[serde(skip)]
    bindings_map: HashMap<String, HashMap<KeyPattern, Action>>,
}

/// Single keybinding entry (keeping for legacy/compat if needed, but we'll use scopes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    pub action: Action,
}

/// Pattern for matching key events
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyPattern {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBindings {
    /// Create default keybindings by loading from the embedded config.json5
    pub fn default() -> Self {
        let content = include_str!("../../.config/config.json5");
        Self::from_json(content).unwrap_or_else(|e| {
            // Fallback to empty if parsing fails (shouldn't happen with valid default config)
            tracing::error!("Failed to parse default keybindings: {}", e);
            Self {
                scopes: HashMap::new(),
                bindings_map: HashMap::new(),
            }
        })
    }

    /// Load from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        let mut bindings: KeyBindings = serde_json::from_str(json)?;
        bindings.rebuild_map();
        Ok(bindings)
    }

    /// Rebuild the internal optimized lookup map
    fn rebuild_map(&mut self) {
        let mut map = HashMap::new();
        for (scope_name, scope_bindings) in &self.scopes {
            let mut scope_map = HashMap::new();
            for (key_str, action) in scope_bindings {
                if let Ok(pattern) = KeyPattern::from_string(key_str) {
                    scope_map.insert(pattern, *action);
                }
            }
            map.insert(scope_name.clone(), scope_map);
        }
        self.bindings_map = map;
    }

    /// Get action for key event in a specific scope
    /// This will also check the "Global" scope if not found in the specific scope.
    pub fn get_action(&self, scope: &str, key: &KeyEvent) -> Option<Action> {
        let pattern = KeyPattern::from_event(key);

        // First check specific scope
        if let Some(scope_map) = self.bindings_map.get(scope) {
            if let Some(action) = scope_map.get(&pattern) {
                return Some(*action);
            }
        }

        // Then check Global scope
        if let Some(global_map) = self.bindings_map.get("Global") {
            if let Some(action) = global_map.get(&pattern) {
                return Some(*action);
            }
        }

        None
    }

    /// Load from JSON config file
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content)
    }

    /// Save to JSON config file
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get all bindings for an action in a scope (for help display)
    pub fn get_keys_for_action(&self, scope: &str, action: Action) -> Vec<String> {
        let mut keys = Vec::new();

        // Check specific scope
        if let Some(scope_bindings) = self.scopes.get(scope) {
            for (key, &bound_action) in scope_bindings {
                if bound_action == action {
                    keys.push(key.clone());
                }
            }
        }

        // Check Global scope
        if let Some(global_bindings) = self.scopes.get("Global") {
            for (key, &bound_action) in global_bindings {
                if bound_action == action {
                    keys.push(key.clone());
                }
            }
        }

        keys
    }

    /// Check for actions that don't have any keybindings in any scope
    pub fn get_unbound_actions(&self) -> Vec<(Action, &'static str)> {
        let mut bound_actions = HashSet::new();
        for scope_bindings in self.scopes.values() {
            for &action in scope_bindings.values() {
                bound_actions.insert(action);
            }
        }

        Action::all()
            .into_iter()
            .filter(|action| !bound_actions.contains(action))
            .map(|action| (action, action.description()))
            .collect()
    }

    /// Validate bindings and return warnings
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Check for duplicate key bindings within each scope
        for (scope_name, scope_bindings) in &self.scopes {
            let mut seen_keys: HashMap<String, Action> = HashMap::new();
            for (key, action) in scope_bindings {
                if let Some(existing_action) = seen_keys.get(key) {
                    warnings.push(format!(
                        "Scope '{}' - Duplicate key '{}': bound to both {:?} and {:?}",
                        scope_name, key, existing_action, action
                    ));
                } else {
                    seen_keys.insert(key.clone(), *action);
                }

                // Check for invalid key patterns
                if KeyPattern::from_string(key).is_err() {
                    warnings.push(format!(
                        "Scope '{}' - Invalid key pattern '{}' for action {:?}",
                        scope_name, key, action
                    ));
                }
            }
        }

        // Check for unbound actions (across all scopes)
        let unbound = self.get_unbound_actions();
        if !unbound.is_empty() {
            warnings.push(format!(
                "Warning: {} action(s) have no keybindings: {}",
                unbound.len(),
                unbound
                    .iter()
                    .map(|(action, _)| format!("{:?}", action))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        warnings
    }
}

impl KeyBinding {
    pub fn new(key: &str, action: Action) -> Self {
        Self {
            key: key.to_string(),
            action,
        }
    }
}

impl KeyPattern {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn from_event(event: &KeyEvent) -> Self {
        Self {
            code: event.code,
            modifiers: event.modifiers,
        }
    }

    /// Parse from string (e.g., "Ctrl+C", "Shift+?", "a")
    pub fn from_string(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('+').collect();

        let mut modifiers = KeyModifiers::empty();
        let key_part = if parts.len() > 1 {
            // Parse modifiers
            for part in &parts[..parts.len() - 1] {
                match part.to_lowercase().as_str() {
                    "ctrl" => modifiers |= KeyModifiers::CONTROL,
                    "alt" => modifiers |= KeyModifiers::ALT,
                    "shift" => modifiers |= KeyModifiers::SHIFT,
                    "cmd" | "command" | "super" => {
                        // Mac Command key maps to SUPER
                        #[cfg(target_os = "macos")]
                        {
                            modifiers |= KeyModifiers::SUPER;
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            modifiers |= KeyModifiers::CONTROL; // Fallback to Ctrl on non-Mac
                        }
                    }
                    _ => return Err(format!("Unknown modifier: {}", part)),
                }
            }
            parts[parts.len() - 1]
        } else {
            // Handle special Shift cases (?, $, etc.)
            if s.len() == 1 {
                let ch = s.chars().next().unwrap();
                if ch.is_uppercase() || "!@#$%^&*()_+{}|:\"<>?".contains(ch) {
                    modifiers |= KeyModifiers::SHIFT;
                }
            }
            parts[0]
        };

        // Parse key code
        let code = match key_part.to_lowercase().as_str() {
            // Special keys first
            "up" | "↑" => KeyCode::Up,
            "down" | "↓" => KeyCode::Down,
            "left" | "←" => KeyCode::Left,
            "right" | "→" => KeyCode::Right,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdown" | "pgdn" => KeyCode::PageDown,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "backspace" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "insert" | "ins" => KeyCode::Insert,
            "space" => KeyCode::Char(' '),

            // Single characters (must come before function key check to avoid matching 'f')
            s if s.len() == 1 => {
                let ch = s.chars().next().unwrap().to_ascii_lowercase();
                KeyCode::Char(ch)
            }

            // Function keys: F1-F12
            s if s.starts_with('f') && s.len() >= 2 && s.len() <= 3 => {
                if let Ok(n) = s[1..].parse::<u8>() {
                    if (1..=12).contains(&n) {
                        KeyCode::F(n)
                    } else {
                        return Err(format!("Invalid function key: {}", s));
                    }
                } else {
                    return Err(format!("Invalid function key: {}", s));
                }
            }

            _ => return Err(format!("Unknown key: {}", key_part)),
        };

        Ok(Self { code, modifiers })
    }

    /// Display as human-readable string
    pub fn to_string(&self) -> String {
        let mut parts = Vec::new();

        if self.modifiers.contains(KeyModifiers::SUPER) {
            #[cfg(target_os = "macos")]
            parts.push("Cmd");
            #[cfg(not(target_os = "macos"))]
            parts.push("Super");
        }
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("Ctrl");
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            parts.push("Alt");
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("Shift");
        }

        let key_str = match self.code {
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Up => "↑".to_string(),
            KeyCode::Down => "↓".to_string(),
            KeyCode::Left => "←".to_string(),
            KeyCode::Right => "→".to_string(),
            KeyCode::PageUp => "PgUp".to_string(),
            KeyCode::PageDown => "PgDn".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::BackTab => "Shift+Tab".to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Del".to_string(),
            KeyCode::F(n) => format!("F{}", n),
            _ => format!("{:?}", self.code),
        };

        parts.push(&key_str);
        parts.join("+")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_pattern_parsing() {
        assert!(KeyPattern::from_string("Ctrl+c").is_ok());
        assert!(KeyPattern::from_string("a").is_ok());
        assert!(KeyPattern::from_string("F1").is_ok());
        assert!(KeyPattern::from_string("Up").is_ok());
        assert!(KeyPattern::from_string("Ctrl+Alt+Delete").is_ok());
    }

    #[test]
    fn test_mac_command_key() {
        let pattern = KeyPattern::from_string("Cmd+c").unwrap();
        #[cfg(target_os = "macos")]
        assert!(pattern.modifiers.contains(KeyModifiers::SUPER));
        #[cfg(not(target_os = "macos"))]
        assert!(pattern.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn test_default_bindings_are_valid() {
        let bindings = KeyBindings::default();
        // Should have at least Global and DataTable scopes
        assert!(bindings.scopes.contains_key("Global"));
        assert!(bindings.scopes.contains_key("DataTable"));

        // Check if a common action is bound
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty());
        assert_eq!(bindings.get_action("DataTable", &key), Some(Action::Quit));
    }

    #[test]
    fn test_unbound_actions() {
        let bindings = KeyBindings::default();
        let unbound = bindings.get_unbound_actions();

        // Each should have a description
        for (_, desc) in unbound {
            assert!(!desc.is_empty());
        }
    }

    #[test]
    fn test_save_and_load() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("keybindings.json");

        let bindings = KeyBindings::default();
        bindings.save_to_file(&path).unwrap();

        let loaded = KeyBindings::load_from_file(&path).unwrap();
        assert_eq!(bindings.scopes.len(), loaded.scopes.len());
    }
}
