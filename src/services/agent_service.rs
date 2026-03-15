//! Agent service for LLM-powered DuckDB SQL query generation.
//!
//! Uses the [Rig](https://docs.rig.rs) library to create an agent that can
//! generate and apply DuckDB SQL queries via tool calling.
//!
//! The `QueryDuckDb` tool does NOT open the database file directly.
//! Instead it sends a `ToolRequest` to the main thread via a sync channel,
//! waits for the main thread to validate and apply the SQL using its
//! existing `DataService` connection, and receives a `ToolResponse`.
//!
//! This avoids the DuckDB exclusive-file-lock conflict between the agent
//! background thread and the main process.

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

// ── Channel message types ────────────────────────────────────────────────

/// A request sent from the `QueryDuckDb` tool to the main thread.
#[derive(Debug, Clone)]
pub enum ToolRequest {
    /// Ask the main thread to validate and apply the given SQL.
    ApplySql(String),
}

/// The main thread's reply to a `ToolRequest`.
#[derive(Debug, Clone)]
pub enum ToolResponse {
    /// The SQL was successfully validated and applied.
    Ok,
    /// Validation or application failed — the error message is returned
    /// to the LLM so it can self-correct.
    Err(String),
}

// ── Tool error ──────────────────────────────────────────────────────────

/// Errors that can occur when executing the DuckDB query tool call.
#[derive(Debug, thiserror::Error)]
pub enum QueryToolError {
    #[error("Channel error: {0}")]
    Channel(String),
}

// ── Tool args / output ──────────────────────────────────────────────────

/// Arguments for the `query_duckdb` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueryArgs {
    /// The DuckDB SQL SELECT query to apply to the dataset table.
    pub query: String,
}

/// The result of applying a DuckDB SQL query.
#[derive(Debug, Serialize, Deserialize)]
pub struct QueryToolOutput {
    /// Whether the query was successfully validated and applied.
    pub success: bool,
    /// Human-readable message describing the result.
    pub message: String,
    /// The SQL that was applied (if successful).
    pub applied_sql: Option<String>,
}

// ── QueryDuckDb tool ────────────────────────────────────────────────────

/// A Rig tool that sends SQL to the main thread for validation and application.
///
/// The tool communicates with the main thread via a pair of sync channels:
/// - `tool_tx` — sends a `ToolRequest` to the main thread
/// - `response_rx` — blocks until the main thread sends back a `ToolResponse`
///
/// The applied SQL is also stored in `applied_query` so the caller can
/// retrieve it once the agent finishes.
#[derive(Serialize, Deserialize)]
pub struct QueryDuckDb {
    table_name: String,

    /// Channel used to send SQL-apply requests to the main thread.
    #[serde(skip)]
    tool_tx: Option<std::sync::mpsc::SyncSender<ToolRequest>>,

    /// Channel used to receive the main thread's reply.
    #[serde(skip)]
    response_rx: Option<Arc<Mutex<std::sync::mpsc::Receiver<ToolResponse>>>>,

    /// Shared slot where the tool records the last successfully applied SQL.
    #[serde(skip)]
    applied_query: Option<Arc<Mutex<Option<String>>>>,
}

impl QueryDuckDb {
    pub fn new(
        table_name: impl Into<String>,
        tool_tx: std::sync::mpsc::SyncSender<ToolRequest>,
        response_rx: std::sync::mpsc::Receiver<ToolResponse>,
        applied_query: Arc<Mutex<Option<String>>>,
    ) -> Self {
        Self {
            table_name: table_name.into(),
            tool_tx: Some(tool_tx),
            response_rx: Some(Arc::new(Mutex::new(response_rx))),
            applied_query: Some(applied_query),
        }
    }
}

impl Tool for QueryDuckDb {
    const NAME: &'static str = "query_duckdb";

    type Error = QueryToolError;
    type Args = QueryArgs;
    type Output = QueryToolOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let parameters = schemars::schema_for!(QueryArgs);

        ToolDefinition {
            name: "query_duckdb".to_string(),
            description: format!(
                "Apply a DuckDB SQL SELECT query to the current dataset. \
                 The query MUST be a SELECT statement targeting the table \
                 '{}'. The result will update what the user sees in the \
                 data table. Return only valid DuckDB SQL.",
                self.table_name
            ),
            parameters: serde_json::to_value(parameters)
                .expect("QueryArgs schema should always serialise"),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let tx = self
            .tool_tx
            .as_ref()
            .ok_or_else(|| QueryToolError::Channel("tool channel not initialised".to_string()))?;
        let rx_arc = self.response_rx.as_ref().ok_or_else(|| {
            QueryToolError::Channel("response channel not initialised".to_string())
        })?;

        // Send the SQL to the main thread for validation + application.
        tx.send(ToolRequest::ApplySql(args.query.clone()))
            .map_err(|e| QueryToolError::Channel(e.to_string()))?;

        // Block until the main thread replies.
        let rx = rx_arc
            .lock()
            .map_err(|e| QueryToolError::Channel(e.to_string()))?;
        let response = rx
            .recv()
            .map_err(|e| QueryToolError::Channel(e.to_string()))?;

        match response {
            ToolResponse::Ok => {
                // Record the applied SQL for the caller.
                if let Some(slot) = &self.applied_query {
                    if let Ok(mut guard) = slot.lock() {
                        *guard = Some(args.query.clone());
                    }
                }
                Ok(QueryToolOutput {
                    success: true,
                    message: "Query validated and applied successfully. \
                              The data table view has been updated."
                        .to_string(),
                    applied_sql: Some(args.query),
                })
            }
            ToolResponse::Err(msg) => Ok(QueryToolOutput {
                success: false,
                message: format!(
                    "Query failed: {}. Please correct the SQL and try again.",
                    msg
                ),
                applied_sql: None,
            }),
        }
    }
}

// ── AgentResponse ───────────────────────────────────────────────────────

/// Response from the agent after processing a user prompt.
#[derive(Debug, Clone)]
pub struct AgentResponse {
    /// The LLM's text response.
    pub response_text: String,
    /// The SQL query that was applied to the dataset (if any).
    pub applied_sql: Option<String>,
    /// Token usage for the request.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    /// The system prompt that was used.
    pub system_prompt: String,
}

// ── AgentService ────────────────────────────────────────────────────────

/// High-level service that wraps Rig agent creation and prompting.
pub struct AgentService;

impl AgentService {
    /// Send a natural-language prompt to the LLM agent and return its
    /// response including any applied SQL query.
    ///
    /// # Arguments
    ///
    /// * `api_key`      – OpenAI API key
    /// * `model`        – Model name (e.g. `"gpt-4o"`)
    /// * `user_prompt`  – The user's natural-language question
    /// * `table_name`   – Name of the dataset table
    /// * `columns`      – Column names in the dataset
    /// * `current_sql`  – The current SQL query active on the dataset
    /// * `tool_tx`      – Sender for tool requests to the main thread
    /// * `response_rx`  – Receiver for the main thread's replies
    /// * `chat_history` – Mutable chat history for multi-turn support
    #[allow(clippy::too_many_arguments)]
    pub async fn prompt(
        api_key: &str,
        model: &str,
        user_prompt: &str,
        table_name: &str,
        columns: &[String],
        current_sql: &str,
        tool_tx: std::sync::mpsc::SyncSender<ToolRequest>,
        response_rx: std::sync::mpsc::Receiver<ToolResponse>,
        chat_history: &mut Vec<rig::completion::Message>,
    ) -> Result<AgentResponse, Box<dyn std::error::Error + Send + Sync>> {
        use rig::completion::Prompt;
        use rig::prelude::CompletionClient;
        use rig::providers::openai;

        let openai_client = openai::Client::new(api_key)?;

        let columns_list = columns.join(", ");
        let preamble = format!(
            "You are a DuckDB SQL assistant. You help users query and analyse \
             their data by writing and executing DuckDB SQL queries.\n\n\
             ## Available Table\n\n\
             Table name: `{table_name}`\n\
             Columns: {columns_list}\n\n\
             ## Current Query\n\n\
             ```sql\n{current_sql}\n```\n\n\
             ## Instructions\n\n\
             - Use the `query_duckdb` tool to apply SQL queries to the dataset.\n\
             - Always use DuckDB SQL syntax.\n\
             - Queries MUST be SELECT statements targeting the table `{table_name}`.\n\
             - When modifying the view, base your query on the current query shown above.\n\
             - If a query fails, analyse the error message and try a corrected query.\n\
             - After applying a query, briefly explain what the query does.\n\
             - If the user asks a question that doesn't require changing the view, \
               just answer it without using the tool."
        );

        // Shared slot for the tool to deposit the applied query.
        let applied_query: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let tool = QueryDuckDb::new(table_name, tool_tx, response_rx, applied_query.clone());

        let agent = openai_client
            .agent(model)
            .preamble(&preamble)
            .tool(tool)
            .build();

        let response = agent
            .prompt(user_prompt)
            .extended_details()
            .max_turns(5)
            .with_history(chat_history)
            .await?;

        // Extract the applied SQL from the shared slot.
        let applied_sql = applied_query.lock().ok().and_then(|guard| guard.clone());

        Ok(AgentResponse {
            response_text: response.output,
            applied_sql,
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
            total_tokens: response.usage.total_tokens,
            system_prompt: preamble,
        })
    }
}
