use jmespath::{Context, Rcvar, Runtime};
use jmespath::functions::{ArgumentType, CustomFunction, Signature};

/// Register all custom JMESPath functions available to the application.
pub fn register_custom_functions(runtime: &mut Runtime) {
    runtime.register_function(
        "keyvalue_to_object",
        Box::new(CustomFunction::new(
            Signature::new(
                vec![
                    ArgumentType::String, // input string
                    ArgumentType::String, // key-value separator
                    ArgumentType::String, // pair separator
                ],
                None
            ),
            Box::new(|args: &[Rcvar], _ctx: &mut Context| {
                // args[0]: input string
                // args[1]: key-value separator
                // args[2]: pair separator
                if args.len() != 3 {
                    // Gracefully return empty object if wrong arity
                    let var = jmespath::Variable::try_from(serde_json::json!({}))?;
                    return Ok(Rcvar::new(var));
                }
                let input: String = args[0]
                    .as_string()
                    .map(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                let kv_sep: String = args[1]
                    .as_string()
                    .map(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                let pair_sep: String = args[2]
                    .as_string()
                    .map(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();

                let mut map = serde_json::Map::new();

                for pair in input.split(&pair_sep) {
                    let trimmed = pair.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let mut split = trimmed.splitn(2, &kv_sep);
                    let key = split.next().unwrap_or("").trim();
                    let value = split.next().unwrap_or("").trim();
                    if !key.is_empty() {
                        map.insert(key.to_string(), serde_json::Value::String(value.to_string()));
                    }
                }

                let var = jmespath::Variable::try_from(serde_json::Value::Object(map))?;
                Ok(Rcvar::new(var))
            }),
        )),
    );

    // to_upper(string) -> string
    runtime.register_function(
        "upper",
        Box::new(CustomFunction::new(
            Signature::new(vec![ArgumentType::String], None),
            Box::new(|args: &[Rcvar], _ctx: &mut Context| {
                let s = args
                    .first()
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| args[0].to_string());
                let upper = s.to_uppercase();
                Ok(Rcvar::new(jmespath::Variable::String(upper)))
            }),
        )),
    );

    // to_lower(string) -> string
    runtime.register_function(
        "lower",
        Box::new(CustomFunction::new(
            Signature::new(vec![ArgumentType::String], None),
            Box::new(|args: &[Rcvar], _ctx: &mut Context| {
                let s = args
                    .first()
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| args[0].to_string());
                let lower = s.to_lowercase();
                Ok(Rcvar::new(jmespath::Variable::String(lower)))
            }),
        )),
    );

    // format(format_string, values_array) -> string
    // Example: format('hello {}', ['world']) => 'hello world'
    runtime.register_function(
        "format",
        Box::new(CustomFunction::new(
            Signature::new(
                vec![
                    ArgumentType::Union(vec![ArgumentType::String, ArgumentType::Null]), // null or string
                    ArgumentType::Array, // array of values to substitute
                ],
                None,
            ),
            Box::new(|args: &[Rcvar], _ctx: &mut Context| {
                if args.len() != 2 {
                    return Ok(Rcvar::new(jmespath::Variable::String(String::new())));
                }

                // If the format string is null, return null
                if args[0].is_null() {
                    return Ok(Rcvar::new(jmespath::Variable::Null));
                }

                // Accept only string for non-null; fall back to to_string for resilience
                let format_string: String = args[0]
                    .as_string()
                    .map(|s| s.as_str().to_string())
                    .unwrap_or_else(|| args[0].to_string());

                let values_opt = args[1].as_array();

                // When values is not an array, treat as empty substitution list.
                let mut result = String::new();
                let mut is_first_part = true;
                let mut value_index: usize = 0;

                let parts = format_string.split("{}");
                for part in parts {
                    if is_first_part {
                        result.push_str(part);
                        is_first_part = false;
                        continue;
                    }

                    // Insert next value if available; otherwise, preserve '{}' literal
                    let substitution = if let Some(values) = values_opt {
                        if let Some(value_var) = values.get(value_index) {
                            let value_str = value_var
                                .as_string()
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| value_var.to_string());
                            value_index += 1;
                            value_str
                        } else {
                            "{}".to_string()
                        }
                    } else {
                        "{}".to_string()
                    };

                    result.push_str(&substitution);
                    result.push_str(part);
                }

                Ok(Rcvar::new(jmespath::Variable::String(result)))
            }),
        )),
    );
}


