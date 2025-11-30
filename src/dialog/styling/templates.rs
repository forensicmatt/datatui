//! Built-in style templates for common data patterns
use crate::dialog::styling::style_set::{
    StyleSet, StyleRule, ApplicationScope, MatchedStyle, ScopeEnum, MergeMode,
    SchemaHint, ColumnMatcher, DynamicStyle, GradientStyle, GradientScale,
    CategoricalStyle,
};
use crate::dialog::filter_dialog::{FilterExpr, FilterCondition, ColumnFilter, CompareOp};
use ratatui::style::{Color, Modifier};

/// Template categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateCategory {
    Errors,
    Status,
    Nulls,
    Numeric,
    Validation,
    Gradient,
    Categorical,
}

impl TemplateCategory {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Errors => "Error Highlighting",
            Self::Status => "Status Colors",
            Self::Nulls => "Null/Empty Highlighting",
            Self::Numeric => "Numeric Visualization",
            Self::Validation => "Data Validation",
            Self::Gradient => "Gradient Heatmap",
            Self::Categorical => "Category Colors",
        }
    }
    
    pub fn description(&self) -> &'static str {
        match self {
            Self::Errors => "Highlight error, fail, and exception values",
            Self::Status => "Color code status columns (success/warning/error)",
            Self::Nulls => "Highlight null, empty, and missing values",
            Self::Numeric => "Visual indicators for numeric ranges",
            Self::Validation => "Highlight potentially invalid data",
            Self::Gradient => "Color gradient based on numeric values (low=blue, high=red)",
            Self::Categorical => "Auto-assign colors to unique category values",
        }
    }
}

/// Get all available template categories
pub fn get_template_categories() -> Vec<TemplateCategory> {
    vec![
        TemplateCategory::Errors,
        TemplateCategory::Status,
        TemplateCategory::Nulls,
        TemplateCategory::Numeric,
        TemplateCategory::Validation,
        TemplateCategory::Gradient,
        TemplateCategory::Categorical,
    ]
}

/// Generate a StyleSet from a template category
pub fn create_template_styleset(category: TemplateCategory) -> StyleSet {
    match category {
        TemplateCategory::Errors => create_error_template(),
        TemplateCategory::Status => create_status_template(),
        TemplateCategory::Nulls => create_null_template(),
        TemplateCategory::Numeric => create_numeric_template(),
        TemplateCategory::Validation => create_validation_template(),
        TemplateCategory::Gradient => create_gradient_template(),
        TemplateCategory::Categorical => create_categorical_template(),
    }
}

/// Create the Error Highlighting template
fn create_error_template() -> StyleSet {
    let error_conditions = vec![
        "error", "fail", "failed", "failure", "exception", "critical",
        "fatal", "crash", "panic", "denied", "rejected", "invalid",
    ];
    
    let rules: Vec<StyleRule> = error_conditions.iter().map(|keyword| {
        StyleRule {
            condition_columns: None, // Check all columns
            match_expr: FilterExpr::Condition(ColumnFilter {
                column: "*".to_string(), // Will be evaluated against all columns
                condition: FilterCondition::Contains {
                    value: keyword.to_string(),
                    case_sensitive: false,
                },
            }),
            style: ApplicationScope {
                scope: ScopeEnum::Row,
                target_columns: None,
                style: MatchedStyle {
                    fg: Some(Color::White),
                    bg: Some(Color::Rgb(139, 0, 0)), // Dark red
                    modifiers: Some(vec![Modifier::BOLD]),
                },
                dynamic_style: None,
            },
            priority: 10,
            merge_mode: MergeMode::Override,
        }
    }).collect();
    
    StyleSet {
        id: "template-errors".to_string(),
        name: "Error Highlighting".to_string(),
        categories: Some(vec!["Templates".to_string(), "Errors".to_string()]),
        tags: Some(vec!["error".to_string(), "fail".to_string(), "exception".to_string()]),
        description: "Highlights rows containing error-related keywords with red background".to_string(),
        yaml_path: None,
        rules,
        schema_hint: None, // Applies to any dataset
    }
}

/// Create the Status Colors template
fn create_status_template() -> StyleSet {
    let rules = vec![
        // Success states - green
        StyleRule {
            condition_columns: Some(vec!["*status*".to_string(), "*state*".to_string()]),
            match_expr: FilterExpr::Or(vec![
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::Contains { value: "success".to_string(), case_sensitive: false },
                }),
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::Contains { value: "complete".to_string(), case_sensitive: false },
                }),
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::Contains { value: "passed".to_string(), case_sensitive: false },
                }),
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::Contains { value: "active".to_string(), case_sensitive: false },
                }),
            ]),
            style: ApplicationScope {
                scope: ScopeEnum::Cell,
                target_columns: None,
                style: MatchedStyle {
                    fg: Some(Color::Rgb(0, 200, 0)),
                    bg: None,
                    modifiers: Some(vec![Modifier::BOLD]),
                },
                dynamic_style: None,
            },
            priority: 5,
            merge_mode: MergeMode::Override,
        },
        // Warning states - yellow
        StyleRule {
            condition_columns: Some(vec!["*status*".to_string(), "*state*".to_string()]),
            match_expr: FilterExpr::Or(vec![
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::Contains { value: "warning".to_string(), case_sensitive: false },
                }),
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::Contains { value: "pending".to_string(), case_sensitive: false },
                }),
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::Contains { value: "waiting".to_string(), case_sensitive: false },
                }),
            ]),
            style: ApplicationScope {
                scope: ScopeEnum::Cell,
                target_columns: None,
                style: MatchedStyle {
                    fg: Some(Color::Rgb(255, 200, 0)),
                    bg: None,
                    modifiers: Some(vec![Modifier::BOLD]),
                },
                dynamic_style: None,
            },
            priority: 5,
            merge_mode: MergeMode::Override,
        },
        // Error states - red
        StyleRule {
            condition_columns: Some(vec!["*status*".to_string(), "*state*".to_string()]),
            match_expr: FilterExpr::Or(vec![
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::Contains { value: "error".to_string(), case_sensitive: false },
                }),
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::Contains { value: "failed".to_string(), case_sensitive: false },
                }),
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::Contains { value: "inactive".to_string(), case_sensitive: false },
                }),
            ]),
            style: ApplicationScope {
                scope: ScopeEnum::Cell,
                target_columns: None,
                style: MatchedStyle {
                    fg: Some(Color::Rgb(255, 80, 80)),
                    bg: None,
                    modifiers: Some(vec![Modifier::BOLD]),
                },
                dynamic_style: None,
            },
            priority: 5,
            merge_mode: MergeMode::Override,
        },
    ];
    
    StyleSet {
        id: "template-status".to_string(),
        name: "Status Colors".to_string(),
        categories: Some(vec!["Templates".to_string(), "Status".to_string()]),
        tags: Some(vec!["status".to_string(), "state".to_string(), "traffic-light".to_string()]),
        description: "Colors status columns: green for success, yellow for warning, red for error".to_string(),
        yaml_path: None,
        rules,
        schema_hint: Some(SchemaHint {
            required_columns: vec![],
            optional_columns: vec![
                ColumnMatcher::Pattern("*status*".to_string()),
                ColumnMatcher::Pattern("*state*".to_string()),
            ],
            min_confidence: 0.3,
        }),
    }
}

/// Create the Null/Empty Highlighting template
fn create_null_template() -> StyleSet {
    let rules = vec![
        StyleRule {
            condition_columns: None,
            match_expr: FilterExpr::Or(vec![
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::IsNull,
                }),
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::IsEmpty,
                }),
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::Equals { value: "null".to_string(), case_sensitive: false },
                }),
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::Equals { value: "none".to_string(), case_sensitive: false },
                }),
                FilterExpr::Condition(ColumnFilter {
                    column: "*".to_string(),
                    condition: FilterCondition::Equals { value: "n/a".to_string(), case_sensitive: false },
                }),
            ]),
            style: ApplicationScope {
                scope: ScopeEnum::Cell,
                target_columns: None,
                style: MatchedStyle {
                    fg: Some(Color::DarkGray),
                    bg: Some(Color::Rgb(40, 40, 40)),
                    modifiers: Some(vec![Modifier::ITALIC]),
                },
                dynamic_style: None,
            },
            priority: 0,
            merge_mode: MergeMode::Override,
        },
    ];
    
    StyleSet {
        id: "template-nulls".to_string(),
        name: "Null/Empty Highlighting".to_string(),
        categories: Some(vec!["Templates".to_string(), "Data Quality".to_string()]),
        tags: Some(vec!["null".to_string(), "empty".to_string(), "missing".to_string()]),
        description: "Highlights null, empty, and missing values with gray italic styling".to_string(),
        yaml_path: None,
        rules,
        schema_hint: None, // Applies to any dataset
    }
}

/// Create the Numeric Visualization template
fn create_numeric_template() -> StyleSet {
    let rules = vec![
        // Negative numbers - red
        StyleRule {
            condition_columns: Some(vec![
                "*amount*".to_string(), "*price*".to_string(), "*value*".to_string(),
                "*total*".to_string(), "*sum*".to_string(), "*balance*".to_string(),
            ]),
            match_expr: FilterExpr::Condition(ColumnFilter {
                column: "*".to_string(),
                condition: FilterCondition::LessThan { value: "0".to_string() },
            }),
            style: ApplicationScope {
                scope: ScopeEnum::Cell,
                target_columns: None,
                style: MatchedStyle {
                    fg: Some(Color::Rgb(255, 100, 100)),
                    bg: None,
                    modifiers: None,
                },
                dynamic_style: None,
            },
            priority: 5,
            merge_mode: MergeMode::Override,
        },
        // Zero values - gray
        StyleRule {
            condition_columns: Some(vec![
                "*amount*".to_string(), "*price*".to_string(), "*value*".to_string(),
                "*total*".to_string(), "*sum*".to_string(), "*count*".to_string(),
            ]),
            match_expr: FilterExpr::Condition(ColumnFilter {
                column: "*".to_string(),
                condition: FilterCondition::Equals { value: "0".to_string(), case_sensitive: true },
            }),
            style: ApplicationScope {
                scope: ScopeEnum::Cell,
                target_columns: None,
                style: MatchedStyle {
                    fg: Some(Color::DarkGray),
                    bg: None,
                    modifiers: None,
                },
                dynamic_style: None,
            },
            priority: 5,
            merge_mode: MergeMode::Override,
        },
        // Large positive numbers - bold green
        StyleRule {
            condition_columns: Some(vec![
                "*amount*".to_string(), "*price*".to_string(), "*value*".to_string(),
            ]),
            match_expr: FilterExpr::Condition(ColumnFilter {
                column: "*".to_string(),
                condition: FilterCondition::GreaterThan { value: "1000".to_string() },
            }),
            style: ApplicationScope {
                scope: ScopeEnum::Cell,
                target_columns: None,
                style: MatchedStyle {
                    fg: Some(Color::Rgb(100, 255, 100)),
                    bg: None,
                    modifiers: Some(vec![Modifier::BOLD]),
                },
                dynamic_style: None,
            },
            priority: 6,
            merge_mode: MergeMode::Override,
        },
    ];
    
    StyleSet {
        id: "template-numeric".to_string(),
        name: "Numeric Visualization".to_string(),
        categories: Some(vec!["Templates".to_string(), "Numeric".to_string()]),
        tags: Some(vec!["number".to_string(), "amount".to_string(), "price".to_string()]),
        description: "Visual indicators for numeric columns: red for negative, gray for zero, green for large values".to_string(),
        yaml_path: None,
        rules,
        schema_hint: Some(SchemaHint {
            required_columns: vec![],
            optional_columns: vec![
                ColumnMatcher::Pattern("*amount*".to_string()),
                ColumnMatcher::Pattern("*price*".to_string()),
                ColumnMatcher::Pattern("*value*".to_string()),
                ColumnMatcher::Pattern("*total*".to_string()),
                ColumnMatcher::Pattern("*balance*".to_string()),
            ],
            min_confidence: 0.3,
        }),
    }
}

/// Create the Data Validation template
fn create_validation_template() -> StyleSet {
    let rules = vec![
        // Email validation - highlight potentially invalid emails
        StyleRule {
            condition_columns: Some(vec!["*email*".to_string()]),
            match_expr: FilterExpr::Condition(ColumnFilter {
                column: "*".to_string(),
                condition: FilterCondition::Not(Box::new(
                    FilterCondition::Regex { 
                        pattern: r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$".to_string(), 
                        case_sensitive: false 
                    }
                )),
            }),
            style: ApplicationScope {
                scope: ScopeEnum::Cell,
                target_columns: None,
                style: MatchedStyle {
                    fg: Some(Color::Rgb(255, 150, 0)),
                    bg: None,
                    modifiers: Some(vec![Modifier::UNDERLINED]),
                },
                dynamic_style: None,
            },
            priority: 5,
            merge_mode: MergeMode::Override,
        },
        // Short strings that might be truncated or invalid
        StyleRule {
            condition_columns: Some(vec!["*name*".to_string(), "*description*".to_string()]),
            match_expr: FilterExpr::Condition(ColumnFilter {
                column: "*".to_string(),
                condition: FilterCondition::StringLength { operator: CompareOp::Lt, length: 3 },
            }),
            style: ApplicationScope {
                scope: ScopeEnum::Cell,
                target_columns: None,
                style: MatchedStyle {
                    fg: Some(Color::Yellow),
                    bg: None,
                    modifiers: None,
                },
                dynamic_style: None,
            },
            priority: 3,
            merge_mode: MergeMode::Override,
        },
        // Very long strings that might indicate data issues
        StyleRule {
            condition_columns: None,
            match_expr: FilterExpr::Condition(ColumnFilter {
                column: "*".to_string(),
                condition: FilterCondition::StringLength { operator: CompareOp::Gt, length: 200 },
            }),
            style: ApplicationScope {
                scope: ScopeEnum::Cell,
                target_columns: None,
                style: MatchedStyle {
                    fg: Some(Color::Cyan),
                    bg: None,
                    modifiers: Some(vec![Modifier::DIM]),
                },
                dynamic_style: None,
            },
            priority: 2,
            merge_mode: MergeMode::Override,
        },
    ];
    
    StyleSet {
        id: "template-validation".to_string(),
        name: "Data Validation".to_string(),
        categories: Some(vec!["Templates".to_string(), "Validation".to_string()]),
        tags: Some(vec!["validation".to_string(), "email".to_string(), "quality".to_string()]),
        description: "Highlights potentially invalid data: bad emails, too short/long strings".to_string(),
        yaml_path: None,
        rules,
        schema_hint: Some(SchemaHint {
            required_columns: vec![],
            optional_columns: vec![
                ColumnMatcher::Pattern("*email*".to_string()),
                ColumnMatcher::Pattern("*name*".to_string()),
            ],
            min_confidence: 0.3,
        }),
    }
}

/// Create the Gradient Heatmap template
fn create_gradient_template() -> StyleSet {
    let rules = vec![
        // Gradient style for numeric columns
        StyleRule {
            condition_columns: Some(vec![
                "*amount*".to_string(), "*price*".to_string(), "*value*".to_string(),
                "*total*".to_string(), "*count*".to_string(), "*score*".to_string(),
                "*percent*".to_string(), "*rate*".to_string(),
            ]),
            match_expr: FilterExpr::And(vec![]), // Always matches - gradient applies to all
            style: ApplicationScope {
                scope: ScopeEnum::Cell,
                target_columns: None,
                style: MatchedStyle {
                    fg: None,
                    bg: None,
                    modifiers: None,
                },
                dynamic_style: Some(DynamicStyle::Gradient(GradientStyle {
                    source_column: "*".to_string(), // Use the matched column
                    min_style: MatchedStyle {
                        fg: None,
                        bg: Some(Color::Rgb(50, 100, 200)), // Blue for low values
                        modifiers: None,
                    },
                    max_style: MatchedStyle {
                        fg: Some(Color::White),
                        bg: Some(Color::Rgb(200, 50, 50)), // Red for high values
                        modifiers: Some(vec![Modifier::BOLD]),
                    },
                    scale: GradientScale::Linear,
                    bounds: None, // Auto-detect from data
                })),
            },
            priority: 5,
            merge_mode: MergeMode::Override,
        },
    ];
    
    StyleSet {
        id: "template-gradient".to_string(),
        name: "Gradient Heatmap".to_string(),
        categories: Some(vec!["Templates".to_string(), "Gradient".to_string()]),
        tags: Some(vec!["gradient".to_string(), "heatmap".to_string(), "numeric".to_string()]),
        description: "Applies color gradient to numeric columns: blue for low, red for high values".to_string(),
        yaml_path: None,
        rules,
        schema_hint: Some(SchemaHint {
            required_columns: vec![],
            optional_columns: vec![
                ColumnMatcher::Pattern("*amount*".to_string()),
                ColumnMatcher::Pattern("*price*".to_string()),
                ColumnMatcher::Pattern("*value*".to_string()),
                ColumnMatcher::Pattern("*score*".to_string()),
            ],
            min_confidence: 0.3,
        }),
    }
}

/// Create the Categorical Colors template
fn create_categorical_template() -> StyleSet {
    let rules = vec![
        // Categorical style for category columns
        StyleRule {
            condition_columns: Some(vec![
                "*category*".to_string(), "*type*".to_string(), "*status*".to_string(),
                "*group*".to_string(), "*class*".to_string(), "*kind*".to_string(),
            ]),
            match_expr: FilterExpr::And(vec![]), // Always matches
            style: ApplicationScope {
                scope: ScopeEnum::Cell,
                target_columns: None,
                style: MatchedStyle {
                    fg: None,
                    bg: None,
                    modifiers: None,
                },
                dynamic_style: Some(DynamicStyle::Categorical(CategoricalStyle {
                    source_column: "*".to_string(), // Use the matched column
                    palette: vec![
                        Color::Rgb(66, 133, 244),   // Blue
                        Color::Rgb(234, 67, 53),    // Red
                        Color::Rgb(251, 188, 5),    // Yellow
                        Color::Rgb(52, 168, 83),    // Green
                        Color::Rgb(154, 160, 166),  // Gray
                        Color::Rgb(255, 112, 67),   // Deep Orange
                        Color::Rgb(156, 39, 176),   // Purple
                        Color::Rgb(0, 188, 212),    // Cyan
                    ],
                    apply_to_fg: true,
                })),
            },
            priority: 5,
            merge_mode: MergeMode::Override,
        },
    ];
    
    StyleSet {
        id: "template-categorical".to_string(),
        name: "Category Colors".to_string(),
        categories: Some(vec!["Templates".to_string(), "Categorical".to_string()]),
        tags: Some(vec!["category".to_string(), "type".to_string(), "group".to_string()]),
        description: "Automatically assigns distinct colors to unique category values".to_string(),
        yaml_path: None,
        rules,
        schema_hint: Some(SchemaHint {
            required_columns: vec![],
            optional_columns: vec![
                ColumnMatcher::Pattern("*category*".to_string()),
                ColumnMatcher::Pattern("*type*".to_string()),
                ColumnMatcher::Pattern("*status*".to_string()),
            ],
            min_confidence: 0.3,
        }),
    }
}

/// Get all templates as StyleSets
pub fn get_all_templates() -> Vec<StyleSet> {
    get_template_categories()
        .into_iter()
        .map(create_template_styleset)
        .collect()
}

