//! AI API client using OpenAI API for testing.
//! This will be replaced with a SpacetimeDB web API or module.

use crate::subcommands::code::state::{ChatMessage, LogEntry, MessageRole};
use crate::subcommands::code::tools::ToolCall;
use crate::util::ModuleLanguage;
use anyhow::{Context, Result};
use futures::stream::Stream;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;

/// OpenAI API client.
#[derive(Clone)]
pub struct AiClient {
    http_client: reqwest::Client,
    api_key: String,
    model: String,
}

impl AiClient {
    /// Create a new AI client using OpenAI API.
    /// The API key should come from the OPENAI_API_KEY environment variable.
    pub fn new(_auth_token: String, _base_url: String) -> Self {
        let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        Self {
            http_client: reqwest::Client::new(),
            api_key,
            model: "gpt-4o".to_string(),
        }
    }

    /// Check if the client has a valid API key.
    pub fn has_api_key(&self) -> bool {
        !self.api_key.is_empty()
    }

    /// Send a chat request and stream the response.
    pub async fn chat_stream(
        &self,
        messages: Vec<ChatMessage>,
        context: AiContext,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<AiStreamEvent>> + Send>>> {
        if !self.has_api_key() {
            anyhow::bail!("OPENAI_API_KEY environment variable not set");
        }

        // Build the messages with system prompt
        let mut api_messages: Vec<OpenAiMessage> = Vec::new();

        // Add system prompt with SpacetimeDB context
        api_messages.push(OpenAiMessage {
            role: "system".to_string(),
            content: Some(build_system_prompt(&context)),
            tool_calls: None,
            tool_call_id: None,
        });

        // Add conversation messages
        for m in messages {
            api_messages.push(OpenAiMessage {
                role: match m.role {
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                    MessageRole::System => "system".to_string(),
                },
                content: Some(m.content),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        let request = OpenAiRequest {
            model: self.model.clone(),
            messages: api_messages,
            stream: true,
            max_tokens: Some(4096),
            tools: Some(get_tool_definitions()),
        };

        let response = self
            .http_client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send chat request to OpenAI")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error ({}): {}", status, error_text);
        }

        let stream = response.bytes_stream();
        let parsed_stream = parse_openai_sse_stream(stream);

        Ok(Box::pin(parsed_stream))
    }

    /// Send a tool result back to the AI and continue the conversation.
    pub async fn send_tool_result(
        &self,
        messages: Vec<ChatMessage>,
        tool_call_id: String,
        _tool_name: String,
        result: String,
        context: AiContext,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<AiStreamEvent>> + Send>>> {
        if !self.has_api_key() {
            anyhow::bail!("OPENAI_API_KEY environment variable not set");
        }

        // Build the messages with system prompt
        let mut api_messages: Vec<OpenAiMessage> = Vec::new();

        // Add system prompt
        api_messages.push(OpenAiMessage {
            role: "system".to_string(),
            content: Some(build_system_prompt(&context)),
            tool_calls: None,
            tool_call_id: None,
        });

        // Add conversation messages
        for m in messages {
            api_messages.push(OpenAiMessage {
                role: match m.role {
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                    MessageRole::System => "system".to_string(),
                },
                content: Some(m.content),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // Add the tool result
        api_messages.push(OpenAiMessage {
            role: "tool".to_string(),
            content: Some(result),
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
        });

        let request = OpenAiRequest {
            model: self.model.clone(),
            messages: api_messages,
            stream: true,
            max_tokens: Some(4096),
            tools: Some(get_tool_definitions()),
        };

        let response = self
            .http_client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send tool result to OpenAI")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error ({}): {}", status, error_text);
        }

        let stream = response.bytes_stream();
        let parsed_stream = parse_openai_sse_stream(stream);

        Ok(Box::pin(parsed_stream))
    }
}

/// Build the system prompt with SpacetimeDB context.
fn build_system_prompt(context: &AiContext) -> String {
    let mut prompt = String::from(
        r#"You are an AI assistant specialized in helping developers with SpacetimeDB, a database that runs WebAssembly modules directly on the database server.

Key SpacetimeDB concepts:
- Tables are defined as Rust structs with #[table(name = table_name, public)] attribute
- Reducers are functions with #[reducer] attribute that modify database state
- Use #[primary_key] and #[auto_inc] for primary key columns
- Clients connect via WebSocket and can subscribe to queries
- The spacetimedb crate provides the core functionality

You have access to file system tools to help the user:
- read_file: Read a file's contents
- write_file: Create or overwrite a file (requires user approval)
- edit_file: Make targeted edits to a file (requires user approval)
- list_files: List files in a directory

When making code changes:
1. First use read_file or list_files to understand the current state
2. Use edit_file for small, targeted changes
3. Use write_file for new files or complete rewrites
4. All write operations require user approval before being applied

"#,
    );

    if let Some(ref db_name) = context.database_name {
        prompt.push_str(&format!("The user is working on a database named: {}\n", db_name));
    }

    if let Some(ref lang) = context.module_language {
        prompt.push_str(&format!("The module is written in: {}\n", lang));
    }

    if let Some(ref logs) = context.recent_logs {
        if !logs.is_empty() {
            prompt.push_str("\nRecent module logs:\n");
            for log in logs.iter().take(10) {
                prompt.push_str(&format!("  {}\n", log));
            }
        }
    }

    prompt.push_str("\nProvide helpful, concise responses. When showing code, use proper markdown code blocks with language tags.");

    prompt
}

/// Get the tool definitions for OpenAI function calling.
fn get_tool_definitions() -> Vec<OpenAiTool> {
    vec![
        OpenAiTool {
            tool_type: "function".to_string(),
            function: OpenAiFunction {
                name: "read_file".to_string(),
                description: "Read the contents of a file. Use this to understand the current code before making changes.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The path to the file to read, relative to the project root"
                        }
                    },
                    "required": ["path"]
                }),
            },
        },
        OpenAiTool {
            tool_type: "function".to_string(),
            function: OpenAiFunction {
                name: "write_file".to_string(),
                description: "Write content to a file, creating it if it doesn't exist or overwriting if it does. Requires user approval.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The path to the file to write, relative to the project root"
                        },
                        "content": {
                            "type": "string",
                            "description": "The content to write to the file"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
        },
        OpenAiTool {
            tool_type: "function".to_string(),
            function: OpenAiFunction {
                name: "edit_file".to_string(),
                description: "Edit a portion of a file by replacing old content with new content. Use this for targeted changes. Requires user approval.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The path to the file to edit, relative to the project root"
                        },
                        "old": {
                            "type": "string",
                            "description": "The exact text to find and replace (must match exactly)"
                        },
                        "new": {
                            "type": "string",
                            "description": "The text to replace it with"
                        }
                    },
                    "required": ["path", "old", "new"]
                }),
            },
        },
        OpenAiTool {
            tool_type: "function".to_string(),
            function: OpenAiFunction {
                name: "list_files".to_string(),
                description: "List files in a directory to understand the project structure.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The path to the directory to list, relative to the project root (use '.' for root)"
                        },
                        "recursive": {
                            "type": "boolean",
                            "description": "Whether to list files recursively (default: false)"
                        }
                    },
                    "required": ["path"]
                }),
            },
        },
    ]
}

/// Context provided to the AI for SpacetimeDB-specific assistance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiContext {
    /// The database name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_name: Option<String>,

    /// The module language (rust, typescript, csharp).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_language: Option<String>,

    /// The current module schema (JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_schema: Option<String>,

    /// Recent log lines for debugging help.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_logs: Option<Vec<String>>,
}

impl AiContext {
    /// Create a new AI context.
    pub fn new(database_name: &str, module_language: ModuleLanguage) -> Self {
        let lang_str = match module_language {
            ModuleLanguage::Rust => "rust",
            ModuleLanguage::Csharp => "csharp",
            ModuleLanguage::Javascript => "typescript",
        };

        Self {
            database_name: Some(database_name.to_string()),
            module_language: Some(lang_str.to_string()),
            module_schema: None,
            recent_logs: None,
        }
    }

    /// Add recent logs to the context.
    pub fn with_recent_logs(mut self, logs: &[LogEntry]) -> Self {
        let log_lines: Vec<String> = logs
            .iter()
            .map(|l| format!("[{}] {}", l.level, l.message))
            .collect();
        self.recent_logs = Some(log_lines);
        self
    }
}

/// OpenAI tool definition.
#[derive(Debug, Clone, Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunction,
}

/// OpenAI function definition.
#[derive(Debug, Clone, Serialize)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// OpenAI API message format.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// OpenAI tool call in message.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiFunctionCall,
}

/// OpenAI function call details.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAiFunctionCall {
    name: String,
    arguments: String,
}

/// OpenAI chat completion request.
#[derive(Debug, Clone, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
}

/// Events from the streaming response.
#[derive(Debug, Clone)]
pub enum AiStreamEvent {
    /// A content chunk.
    Content(String),
    /// A tool call from the AI.
    ToolCall(ToolCall),
    /// The response is complete.
    Done(Option<Usage>),
    /// An error occurred.
    Error(String),
}

/// Token usage information.
#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// OpenAI streaming response chunk.
#[derive(Debug, Clone, Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
}

/// OpenAI streaming response choice.
#[derive(Debug, Clone, Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiDelta,
    finish_reason: Option<String>,
}

/// OpenAI delta content in streaming response.
#[derive(Debug, Clone, Deserialize)]
struct OpenAiDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiDeltaToolCall>>,
}

/// OpenAI tool call delta in streaming response.
#[derive(Debug, Clone, Deserialize)]
struct OpenAiDeltaToolCall {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAiDeltaFunction>,
}

/// OpenAI function call delta.
#[derive(Debug, Clone, Deserialize)]
struct OpenAiDeltaFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// State for accumulating tool calls across streaming chunks.
#[derive(Debug, Clone, Default)]
struct ToolCallAccumulator {
    tool_calls: HashMap<usize, AccumulatingToolCall>,
}

#[derive(Debug, Clone, Default)]
struct AccumulatingToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl ToolCallAccumulator {
    fn process_delta(&mut self, delta_calls: &[OpenAiDeltaToolCall]) {
        for delta in delta_calls {
            let entry = self.tool_calls.entry(delta.index).or_default();

            if let Some(ref id) = delta.id {
                entry.id = id.clone();
            }

            if let Some(ref func) = delta.function {
                if let Some(ref name) = func.name {
                    entry.name = name.clone();
                }
                if let Some(ref args) = func.arguments {
                    entry.arguments.push_str(args);
                }
            }
        }
    }

    fn into_tool_calls(self) -> Vec<ToolCall> {
        let mut calls: Vec<_> = self.tool_calls.into_iter().collect();
        calls.sort_by_key(|(idx, _)| *idx);

        calls
            .into_iter()
            .filter_map(|(_, acc)| {
                if acc.id.is_empty() || acc.name.is_empty() {
                    return None;
                }

                let input: serde_json::Value = serde_json::from_str(&acc.arguments)
                    .unwrap_or(serde_json::Value::Object(Default::default()));

                Some(ToolCall {
                    id: acc.id,
                    name: acc.name,
                    input,
                })
            })
            .collect()
    }
}

/// Parse OpenAI SSE stream into AI events.
fn parse_openai_sse_stream<S>(stream: S) -> impl Stream<Item = Result<AiStreamEvent>>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
{
    async_stream::try_stream! {
        let mut buffer = String::new();
        let mut tool_accumulator = ToolCallAccumulator::default();
        let mut has_tool_calls = false;

        futures::pin_mut!(stream);

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.context("Failed to read stream chunk")?;
            let chunk_str = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_str);

            // Process complete SSE events (lines starting with "data: ")
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if data.trim() == "[DONE]" {
                        // Emit any accumulated tool calls before Done
                        if has_tool_calls {
                            let tool_calls = std::mem::take(&mut tool_accumulator).into_tool_calls();
                            for tool_call in tool_calls {
                                yield AiStreamEvent::ToolCall(tool_call);
                            }
                        }
                        yield AiStreamEvent::Done(None);
                        continue;
                    }

                    match serde_json::from_str::<OpenAiStreamChunk>(data) {
                        Ok(chunk) => {
                            for choice in chunk.choices {
                                // Handle content
                                if let Some(content) = choice.delta.content {
                                    if !content.is_empty() {
                                        yield AiStreamEvent::Content(content);
                                    }
                                }

                                // Handle tool calls
                                if let Some(ref delta_calls) = choice.delta.tool_calls {
                                    has_tool_calls = true;
                                    tool_accumulator.process_delta(delta_calls);
                                }

                                // Check for finish reason
                                if let Some(ref reason) = choice.finish_reason {
                                    if reason == "tool_calls" {
                                        // Emit accumulated tool calls
                                        let tool_calls = std::mem::take(&mut tool_accumulator).into_tool_calls();
                                        for tool_call in tool_calls {
                                            yield AiStreamEvent::ToolCall(tool_call);
                                        }
                                        has_tool_calls = false;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse OpenAI chunk: {} - data: {}", e, data);
                        }
                    }
                }
            }
        }
    }
}
