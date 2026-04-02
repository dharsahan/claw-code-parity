//! API client for TUI streaming

use std::sync::mpsc;

use api::{
    resolve_startup_auth_source, ContentBlockDelta, MessageRequest, OutputContentBlock,
    ProviderClient, ProviderKind, StreamEvent as ApiStreamEvent, ToolChoice, ToolDefinition,
};
use runtime::TokenUsage;

use super::events::{Event, StreamEvent};

/// API client for TUI that sends streaming events through a channel
#[derive(Clone)]
pub struct TuiApiClient {
    client: ProviderClient,
    model: String,
    tools: Vec<ToolDefinition>,
}

impl TuiApiClient {
    /// Create a new TUI API client
    pub fn new(
        model: String,
        tools: Vec<ToolDefinition>,
        provider_override: Option<ProviderKind>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let auth = resolve_startup_auth_source(|| Ok(None)).ok();
        let client = ProviderClient::from_model_with_override_and_auth(&model, provider_override, auth)?;
        
        Ok(Self {
            client,
            model,
            tools,
        })
    }

    /// Update the model
    #[allow(dead_code)]
    pub fn set_model(&mut self, model: String, provider_override: Option<ProviderKind>) -> Result<(), Box<dyn std::error::Error>> {
        let auth = resolve_startup_auth_source(|| Ok(None)).ok();
        self.client = ProviderClient::from_model_with_override_and_auth(&model, provider_override, auth)?;
        self.model = model;
        Ok(())
    }

    /// Stream a message and send events to the provided sender
    pub async fn stream_message(
        &self,
        messages: Vec<api::InputMessage>,
        system: Option<String>,
        event_tx: mpsc::Sender<Event>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let request = MessageRequest {
            model: self.model.clone(),
            max_tokens: max_tokens_for_model(&self.model),
            messages,
            system,
            tools: if self.tools.is_empty() { None } else { Some(self.tools.clone()) },
            tool_choice: if self.tools.is_empty() { None } else { Some(ToolChoice::Auto) },
            stream: true,
        };

        let mut stream = self.client.stream_message(&request).await?;
        let mut pending_tool: Option<(String, String, String)> = None;

        while let Some(event) = stream.next_event().await? {
            match event {
                ApiStreamEvent::MessageStart(start) => {
                    for block in start.message.content {
                        self.handle_content_block(block, &event_tx, &mut pending_tool)?;
                    }
                }
                ApiStreamEvent::ContentBlockStart(start) => {
                    self.handle_content_block(start.content_block, &event_tx, &mut pending_tool)?;
                }
                ApiStreamEvent::ContentBlockDelta(delta) => match delta.delta {
                    ContentBlockDelta::TextDelta { text } => {
                        if !text.is_empty() {
                            let _ = event_tx.send(Event::Stream(StreamEvent::TextDelta(text)));
                        }
                    }
                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                        if let Some((_, _, input)) = &mut pending_tool {
                            input.push_str(&partial_json);
                        }
                    }
                    ContentBlockDelta::ThinkingDelta { .. }
                    | ContentBlockDelta::SignatureDelta { .. } => {}
                },
                ApiStreamEvent::ContentBlockStop(_) => {
                    if let Some((id, _name, input)) = pending_tool.take() {
                        let _ = event_tx.send(Event::Stream(StreamEvent::ToolUseInput {
                            id,
                            input,
                        }));
                    }
                }
                ApiStreamEvent::MessageDelta(delta) => {
                    let _ = event_tx.send(Event::Stream(StreamEvent::Usage(TokenUsage {
                        input_tokens: delta.usage.input_tokens,
                        output_tokens: delta.usage.output_tokens,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                    })));
                }
                ApiStreamEvent::MessageStop(_) => {
                    let _ = event_tx.send(Event::Stream(StreamEvent::Done));
                }
            }
        }

        Ok(())
    }

    fn handle_content_block(
        &self,
        block: OutputContentBlock,
        event_tx: &mpsc::Sender<Event>,
        pending_tool: &mut Option<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match block {
            OutputContentBlock::Text { text } => {
                if !text.is_empty() {
                    let _ = event_tx.send(Event::Stream(StreamEvent::TextDelta(text)));
                }
            }
            OutputContentBlock::ToolUse { id, name, input } => {
                let _ = event_tx.send(Event::Stream(StreamEvent::ToolUseStart {
                    id: id.clone(),
                    name: name.clone(),
                }));
                *pending_tool = Some((id, name, input.to_string()));
            }
            OutputContentBlock::Thinking { .. } | OutputContentBlock::RedactedThinking { .. } => {}
        }
        Ok(())
    }
}

fn max_tokens_for_model(model: &str) -> u32 {
    if model.contains("opus") {
        32_000
    } else {
        64_000
    }
}
