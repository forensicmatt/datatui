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
    /// Ask the main thread to run a read-only query and return formatted results.
    QueryForContext { sql: String, limit: usize },
}

/// The main thread's reply to a `ToolRequest`.
#[derive(Debug, Clone)]
pub enum ToolResponse {
    /// The SQL was successfully validated and applied.
    Ok,
    /// Validation or application failed — the error message is returned
    /// to the LLM so it can self-correct.
    Err(String),
    /// Formatted tabular results from a QueryForContext request.
    QueryResult(String),
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
            ToolResponse::QueryResult(_) => {
                // Should not happen for this tool
                Ok(QueryToolOutput {
                    success: false,
                    message: "Unexpected response type".to_string(),
                    applied_sql: None,
                })
            }
        }
    }
}

// ── QueryForContext tool ────────────────────────────────────────────────

/// Arguments for the `query_for_context` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueryContextArgs {
    /// The DuckDB SQL SELECT query to run (e.g. for aggregations or finding specifics).
    pub query: String,
    /// The maximum number of rows to return. Capped at 50 if higher or omitted.
    pub limit: Option<usize>,
}

/// The result of querying DuckDB for context.
#[derive(Debug, Serialize, Deserialize)]
pub struct QueryContextOutput {
    /// Whether the query was successfully executed.
    pub success: bool,
    /// The result of the query, typically a markdown table or an error message.
    pub result: String,
}

/// A Rig tool that sends a read-only SQL query to the main thread to get formatted
/// results back as context, helping the agent answer questions.
#[derive(Serialize, Deserialize)]
pub struct QueryForContext {
    table_name: String,

    /// Channel used to send SQL requests to the main thread.
    #[serde(skip)]
    tool_tx: Option<std::sync::mpsc::SyncSender<ToolRequest>>,

    /// Channel used to receive the main thread's reply (formatted table string).
    #[serde(skip)]
    response_rx: Option<Arc<Mutex<std::sync::mpsc::Receiver<ToolResponse>>>>,
}

impl QueryForContext {
    pub fn new(
        table_name: impl Into<String>,
        tool_tx: std::sync::mpsc::SyncSender<ToolRequest>,
        response_rx: std::sync::mpsc::Receiver<ToolResponse>,
    ) -> Self {
        Self {
            table_name: table_name.into(),
            tool_tx: Some(tool_tx),
            response_rx: Some(Arc::new(Mutex::new(response_rx))),
        }
    }
}

impl Tool for QueryForContext {
    const NAME: &'static str = "query_for_context";

    type Error = QueryToolError;
    type Args = QueryContextArgs;
    type Output = QueryContextOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let parameters = schemars::schema_for!(QueryContextArgs);

        ToolDefinition {
            name: Self::NAME.to_string(),
            description: format!(
                "Run a read-only DuckDB SQL SELECT query against table '{}' to gather context \
                 in order to answer a user's question (e.g. finding the highest value, \
                 averages, or specific rows). Does NOT modify the dataset view. \
                 Results are returned as a markdown table. \
                 Keep limit small (max 50) and use SQL aggregations where possible.",
                self.table_name
            ),
            parameters: serde_json::to_value(parameters)
                .expect("QueryContextArgs schema should always serialise"),
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

        let limit = args.limit.unwrap_or(50).min(50); // Cap at 50

        // Send the SQL to the main thread for execution.
        tx.send(ToolRequest::QueryForContext {
            sql: args.query.clone(),
            limit,
        })
        .map_err(|e| QueryToolError::Channel(e.to_string()))?;

        // Block until the main thread replies.
        let rx = rx_arc
            .lock()
            .map_err(|e| QueryToolError::Channel(e.to_string()))?;
        let response = rx
            .recv()
            .map_err(|e| QueryToolError::Channel(e.to_string()))?;

        match response {
            ToolResponse::QueryResult(result_table) => Ok(QueryContextOutput {
                success: true,
                result: result_table,
            }),
            ToolResponse::Err(msg) => Ok(QueryContextOutput {
                success: false,
                result: format!("Query failed: {}", msg),
            }),
            ToolResponse::Ok => Ok(QueryContextOutput {
                success: false,
                result: "Unexpected response type (Ok)".to_string(),
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
             - Use the `query_duckdb` tool to apply SQL queries that change what the user sees in the data table.\n\
             - Use the `query_for_context` tool to run a read-only SQL query to gather information to answer the user's question, without changing the view.\n\
             - Always use DuckDB SQL syntax.\n\
             - Queries MUST be SELECT statements targeting the table `{table_name}`.\n\
             - For `query_duckdb`, base your query on the Current Query. For `query_for_context`, you can construct any SELECT query against `{table_name}` to get the needed data.\n\
             - If a query fails, analyse the error message and try a corrected query.\n\
             - After applying a query (`query_duckdb`), briefly explain what the query does.\n\
             - If the user asks a factual question, use `query_for_context` to find the answer and then provide the answer."
        );

        // Shared slot for the apply tool to deposit the applied query.
        let applied_query: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        // We need to clone channels for both tools
        let tool_tx_apply = tool_tx.clone();
        let tool_tx_context = tool_tx;

        // The receiver is already behind a Mutex inside the tools, but we need
        // an Arc wrapping the receiver to share it between tools.
        // The original method passes the raw receiver in `response_rx`.
        let shared_rx = Arc::new(Mutex::new(response_rx));

        // Instantiate both tools
        // We modify the tool constructors to accept the Arc directly for convenience in sharing
        let query_apply_tool = QueryDuckDb {
            table_name: table_name.to_string(),
            tool_tx: Some(tool_tx_apply),
            response_rx: Some(shared_rx.clone()),
            applied_query: Some(applied_query.clone()),
        };

        let query_context_tool = QueryForContext {
            table_name: table_name.to_string(),
            tool_tx: Some(tool_tx_context),
            response_rx: Some(shared_rx),
        };

        let agent = openai_client
            .agent(model)
            .preamble(&preamble)
            .tool(query_apply_tool)
            .tool(query_context_tool)
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
