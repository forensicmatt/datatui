# LLM Configuration Implementation Plan

##Overview
Add LLM capabilities to the application, starting with provider configuration for OpenAI, Azure OpenAI, and Ollama.

## Phase 1: Core Data Structures and Service Layer

### 1.1 Create LLM Configuration Structures
**File**: `src/core/llm_config.rs`
- `LlmProvider` enum (OpenAI, Azure, Ollama)
- `OpenAiConfig` struct (api_key, base_url)
- `AzureOpenAiConfig` struct (api_key, base_url, api_version)
- `OllamaConfig` struct (host)
- `LlmSettings` struct to hold all configs and default provider
- Serialization/Deserialization traits
- Default implementations

### 1.2 Create LLM Service
**File**: `src/services/llm_service.rs`
- `LlmService` struct
- Methods for loading/saving LLM configuration to TOML file
- Methods for getting/setting provider configs
- Method for setting default provider
- Validation logic

### 1.3 Update Services Module
**File**: `src/services/mod.rs`
- Export `llm_service`

### 1.4 Update Core Module
**File**: `src/core/mod.rs`
- Export `llm_config`

## Phase 2: Actions and Keybindings

### 2.1 Add Actions
**File**: `src/tui/action.rs`
- `OpenLlmManagementDialog`
- `SaveLlmConfig`
- Add to all() method
- Add descriptions and categories

### 2.2 Add Keybindings
**File**: `.config/config.json5`
- Add Global keybinding for OpenLlmManagementDialog (e.g., Ctrl+L)
- Add LlmDialog scope for dialog-specific actions

## Phase 3: Dialog Components

### 3.1 OpenAI Configuration Dialog
**File**: `src/tui/components/openai_config_dialog.rs`
- Form with fields: API Key, Base URL
- Field navigation (Tab, Up/Down)
- Cursor management
- Text editing (insert, delete, backspace)
- Clipboard support (paste, copy)
- Instructions area
- Save on Enter, Cancel on Escape
- Theme integration

### 3.2 Azure OpenAI Configuration Dialog
**File**: `src/tui/components/azure_openai_config_dialog.rs`
- Form with fields: API Key, Base URL, API Version
- Same features as OpenAI dialog

### 3.3 Ollama Configuration Dialog
**File**: `src/tui/components/ollama_config_dialog.rs`
- Form with field: Host URL
- Same features as OpenAI dialog

### 3.4 LLM Management Dialog (Main Entry Point)
**File**: `src/tui/components/llm_management_dialog.rs`
- Provider selection list (Azure OpenAI, OpenAI, Ollama)
- Navigation between provider selection and configuration
- Launch provider-specific dialogs
- Display configuration status (configured/not configured)
- Default provider selection
- Instructions area
- Delegate to specific dialogs

### 3.5 Update Components Module
**File**: `src/tui/components/mod.rs`
- Export all new dialog components

## Phase 4: Integration

### 4.1 Update App State
**File**: `src/tui/app.rs`
- Add `llm_service: LlmService` field
- Add `llm_management_dialog: Option<LlmManagementDialog>`
- Initialize in `App::new()`
- Load existing config on startup

### 4.2 Update App Rendering
**File**: `src/tui/app.rs` (`render` method)
- Render llm_management_dialog if present (on top of other dialogs)

### 4.3 Update App Event Handling
**File**: `src/tui/app.rs` (`handle_action` method)
- Handle `OpenLlmManagementDialog` action
- Handle `SaveLlmConfig` action
- Handle dialog close with configuration updates

### 4.4 Update App Key Event Handling
**File**: `src/tui/app.rs` (`handle_key_event` method)
- Delegate to llm_management_dialog when present

## Phase 5: Configuration Persistence

### 5.1 Config File Location
- Store in `~/.datatui-llm-settings.toml` (following the old pattern)
- Auto-create with defaults if missing

### 5.2 Config Structure
```toml
default_provider = "OpenAI"  # or "Azure" or "Ollama"

[openai]
api_key = ""
base_url = "https://api.openai.com/v1"

[azure]
api_key = ""
base_url = ""
api_version = "2024-02-15-preview"

[ollama]
host = "http://localhost:11434"
```

## Phase 6: Testing and Verification

### 6.1 Manual Testing
- Open LLM management dialog
- Configure each provider
- Save configurations
- Verify persistence
- Verify default provider selection
- Test all navigation and editing features

### 6.2 Build Verification
```bash
cargo check
cargo build
```

## Implementation Order

1. **Phase 1**: Core structures and service (foundation)
2. **Phase 2**: Actions (needed for dialogs)
3. **Phase 3**: Dialog components (UI layer)
4. **Phase 4**: Integration (connect everything)
5. **Phase 5**: Configuration persistence (already in service)
6. **Phase 6**: Testing

## Notes

- Follow existing dialog patterns (SortDialog, ColumnWidthDialog, etc.)
- Use Theme for all styling (no hardcoded colors)
- Implement Focusable trait for all dialogs
- Handle focus state for border styling
- Support instructions toggle with Ctrl+I
- Use Component trait for all dialogs
- Configs are managed via service APIs to support multiple interfaces
- Default values from environment variables where applicable
