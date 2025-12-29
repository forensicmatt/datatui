pub mod style_set;
pub mod style_set_manager;
pub mod style_set_manager_dialog;
pub mod style_rule_editor_dialog;
pub mod style_set_browser_dialog;
pub mod style_set_editor_dialog;
pub mod application_scope_editor_dialog;
pub mod color_picker_dialog;
pub mod templates;

pub use style_set::{
    StyleSet, StyleRule, MatchedStyle, 
    GrepCapture, ApplicationScope, StyleApplication,
    Condition, ConditionalStyle, StyleLogic,
    MergeMode, GradientStyle, GradientScale, CategoricalStyle,
    SchemaHint, ColumnMatcher, ExpectedType,
    matches_column,
};
pub use templates::{get_all_templates, get_template_categories, create_template_styleset, TemplateCategory};
pub use style_set_manager::StyleSetManager;
pub use style_set_manager_dialog::StyleSetManagerDialog;
pub use style_rule_editor_dialog::StyleRuleEditorDialog;
pub use style_set_browser_dialog::StyleSetBrowserDialog;
pub use style_set_editor_dialog::StyleSetEditorDialog;
pub use application_scope_editor_dialog::ApplicationScopeEditorDialog;
pub use color_picker_dialog::ColorPickerDialog;
