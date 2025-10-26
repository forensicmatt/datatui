use polars::prelude::*;
use polars_plan::dsl::udf::UserDefinedFunction;
use polars_plan::dsl::GetOutput;
use polars_sql::SQLContext;
use polars_sql::function_registry::FunctionRegistry;
use lazy_static::lazy_static;
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
// use crate::providers::openai::Client as OpenAIClient;

// Reduce type complexity with a clear alias for the embeddings provider closure type
type EmbeddingsProvider = Arc<dyn Fn(&[String]) -> PolarsResult<Vec<Vec<f32>>> + Send + Sync>;

fn upper_impl(columns: &mut [Column]) -> PolarsResult<Option<Column>> {
	if columns.len() != 1 {
		return Err(PolarsError::ComputeError(
			"upper function expects exactly one argument".into(),
		));
	}
	let s = columns[0].as_materialized_series();
	let s = if s.dtype() == &DataType::String { s } else { &s.cast(&DataType::String)? };
	let out_series = s.str()?.to_uppercase().into_series();
	Ok(Some(out_series.into_column()))
}

pub fn register_all(ctx: &mut SQLContext) -> PolarsResult<()> {
	// Build UDF for upper(text) -> text
	let udf = UserDefinedFunction::new(
		"upper".into(),
		GetOutput::from_type(DataType::String),
		upper_impl,
	);
	ctx.registry_mut().register("upper", udf)?;

	// // Build UDF for embed(text) -> List(Float32)
	// let embed_udf = UserDefinedFunction::new(
	// 	"embed".into(),
	// 	GetOutput::from_type(DataType::List(Box::new(DataType::Float32))),
	// 	embed_impl,
	// );
	// ctx.registry_mut().register("embed", embed_udf)?;
	Ok(())
}

// Custom UDF function registry to support dynamic registration
#[derive(Default)]
struct MyFunctionRegistry {
    functions: HashMap<String, UserDefinedFunction>,
}

impl FunctionRegistry for MyFunctionRegistry {
    fn register(&mut self, name: &str, fun: UserDefinedFunction) -> PolarsResult<()> {
        self.functions.insert(name.to_string(), fun);
        Ok(())
    }

    fn get_udf(&self, name: &str) -> PolarsResult<Option<UserDefinedFunction>> {
        Ok(self.functions.get(name).cloned())
    }

    fn contains(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }
}

/// Create a SQLContext configured with a custom registry that supports registering UDFs.
pub fn new_sql_context() -> SQLContext {
    SQLContext::new()
		.with_function_registry(Arc::new(MyFunctionRegistry::default()))
}

// // Embeddings provider configuration
// // The provider takes a slice of unique strings and returns an embedding vector per input, in order.
// lazy_static! {
// 	static ref EMBEDDINGS_PROVIDER: RwLock<Option<EmbeddingsProvider>> = RwLock::new(None);
// }

// /// Set the embeddings provider used by the `embed` SQL function.
// pub fn set_embeddings_provider(provider: EmbeddingsProvider) {
// 	*EMBEDDINGS_PROVIDER.write().expect("EMBEDDINGS_PROVIDER poisoned") = Some(provider);
// }

// /// Convenience: configure the embeddings provider to use OpenAI's embeddings for given client and model.
// /// The closure runs Blocking HTTP; intended to be called from non-async SQL UDF context.
// pub fn set_openai_embeddings_provider(client: OpenAIClient, model: Option<String>) {
// 	let selected_model = model.unwrap_or_else(|| "text-embedding-3-small".to_string());
// 	let provider = move |inputs: &[String]| -> PolarsResult<Vec<Vec<f32>>> {
// 		match client.generate_embeddings(inputs, Some(&selected_model), None) {
// 			Ok(v) => Ok(v),
// 			Err(e) => Err(PolarsError::ComputeError(format!("OpenAI embeddings error: {e}").into())),
// 		}
// 	};
// 	set_embeddings_provider(Arc::new(provider));
// }

// fn embed_impl(columns: &mut [Column]) -> PolarsResult<Option<Column>> {
// 	if columns.len() != 1 {
// 		return Err(PolarsError::ComputeError("embed function expects exactly one argument".into()));
// 	}
// 	let mut s = columns[0].as_materialized_series().clone();
// 	if s.dtype() != &DataType::String {
// 		s = s.cast(&DataType::String)?;
// 	}

// 	// Extract texts and build mapping unique_value -> unique_index
// 	let len = s.len();
// 	let mut row_texts: Vec<Option<String>> = Vec::with_capacity(len);
// 	let mut unique_index: HashMap<String, usize> = HashMap::new();
// 	let mut uniques: Vec<String> = Vec::new();
// 	for i in 0..len {
// 		let av_res = s.get(i);
// 		if let Ok(av) = av_res {
// 			if av.is_null() {
// 				row_texts.push(None);
// 				continue;
// 			}
// 			let text_val = av.str_value().to_string();
// 			row_texts.push(Some(text_val.clone()));
// 			if !unique_index.contains_key(&text_val) {
// 				let idx = uniques.len();
// 				unique_index.insert(text_val.clone(), idx);
// 				uniques.push(text_val);
// 			}
// 		} else {
// 			row_texts.push(None);
// 			continue;
// 		}
// 	}

// 	// Acquire provider
// 	let provider = EMBEDDINGS_PROVIDER
// 		.read()
// 		.expect("EMBEDDINGS_PROVIDER poisoned")
// 		.as_ref()
// 		.cloned()
// 		.ok_or_else(|| PolarsError::ComputeError("embed function provider not configured".into()))?;

// 	// Compute embeddings only for unique values
// 	let unique_embeddings: Vec<Vec<f32>> = (provider)(&uniques)?;
// 	if unique_embeddings.len() != uniques.len() {
// 		return Err(PolarsError::ComputeError("embeddings provider returned wrong length".into()));
// 	}

// 	// Map back to each row
// 	let row_embeddings_iter = row_texts.into_iter().map(|opt_text| {
// 		opt_text.map(|t| {
// 			let idx = unique_index.get(&t).copied().unwrap();
// 			let v: &Vec<f32> = &unique_embeddings[idx];
// 			Series::new(PlSmallStr::EMPTY, v.clone())
// 		})
// 	});
// 	let mut lc: ListChunked = row_embeddings_iter.collect();
// 	lc.rename(PlSmallStr::EMPTY);
// 	Ok(Some(lc.into_series().into_column()))
// }
