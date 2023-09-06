use async_openai::types as aot;

/// Temperature used in all requests.
const TEMPERATURE: f32 = 0.0;

/// Minimum number of tokens to be able to generate in the completion.
const MIN_COMPLETION_TOKENS: usize = 512;

/// Available `OpenAI` models sorted by price.
const MODELS: [&str; 4] = [
    "gpt-3.5-turbo",     // $0.0015 / 1K tokens
    "gpt-3.5-turbo-16k", // $0.003  / 1K tokens
    "gpt-4",             // $0.03   / 1K tokens
    "gpt-4-32k",         // $0.06   / 1K tokens
];

/// Check if the given model has a large enough context length for the given
/// messages.
///
/// # Errors
/// If the model could not be retrieved.
#[inline]
fn messages_fit_model(
    model: &str,
    messages: &[aot::ChatCompletionRequestMessage],
) -> anyhow::Result<bool> {
    Ok(
        tiktoken_rs::async_openai::get_chat_completion_max_tokens(model, messages)?
            >= MIN_COMPLETION_TOKENS,
    )
}

/// Find the cheapest model with large enough context length for the given
/// messages.
///
/// If no model with large enough context length can be found,
/// this returns [`None`].
#[inline]
fn choose_model(messages: &[aot::ChatCompletionRequestMessage]) -> Option<&'static str> {
    MODELS.into_iter().find(|model| {
        messages_fit_model(model, messages)
            .expect("model retrieval of known models should never fail")
    })
}

/// Create a user message for the given input.
///
/// # Errors
/// If the created message could not fit the cheapest model alone.
#[inline]
fn create_user_message(input: &str) -> anyhow::Result<aot::ChatCompletionRequestMessage> {
    let input = input.trim();
    let messages = [aot::ChatCompletionRequestMessageArgs::default()
        .role(aot::Role::User)
        .content(input)
        .build()?];
    anyhow::ensure!(
        messages_fit_model(MODELS[0], &messages)?,
        "user input should fit the cheapest model alone"
    );
    let [message] = messages;
    Ok(message)
}

/// Get chat messages ending in the given new messages,
/// essentially building context to them.
#[inline]
fn create_chat_messages(
    new_messages: &[aot::ChatCompletionRequestMessage],
) -> Vec<aot::ChatCompletionRequestMessage> {
    // TODO: actually get previous chat messages,
    // and split/prune to fit the context,
    // this should
    // always produce a valid context for at least *one* of the MODELS.
    new_messages.into()
}

/// Create an `OpenAI` request.
///
/// # Errors
/// If a model could not be chosen for the given messages.
#[inline]
fn create_request(
    messages: Vec<aot::ChatCompletionRequestMessage>,
) -> anyhow::Result<aot::CreateChatCompletionRequest> {
    use anyhow::Context as _;

    let model = choose_model(&messages).context(
        "no model with large enough context length could be found for the given messages",
    )?;
    Ok(aot::CreateChatCompletionRequestArgs::default()
        .temperature(TEMPERATURE)
        .messages(messages)
        .model(model)
        // TODO: function specifications will be added in the future here
        .functions([aot::ChatCompletionFunctionsArgs::default()
            .name("get_current_weather")
            .description("Get the current weather in a given location")
            .parameters(serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city and state, e.g. San Francisco, CA",
                    },
                    "unit": {
                        "type": "string",
                        "enum": ["celsius", "fahrenheit"],
                    },
                },
                "required": ["location"],
            }))
            .build()?])
        .build()?)
}

#[inline]
async fn create_response(
    request: aot::CreateChatCompletionRequest,
) -> Result<aot::ChatCompletionResponseStream, async_openai::error::OpenAIError> {
    async_openai::Client::new()
        .chat()
        .create_stream(request)
        .await
}

#[inline]
async fn create_assistant_message(
    mut response: aot::ChatCompletionResponseStream,
) -> anyhow::Result<aot::ChatCompletionRequestMessage> {
    use std::fmt::Write as _;

    use futures::StreamExt as _;
    use tokio::io::AsyncWriteExt as _;

    let mut stdout = tokio::io::stdout();
    let mut content_buffer = String::new();
    let mut function_call_name = String::new();
    let mut function_call_arguments_buffer = String::new();
    while let Some(result) = response.next().await {
        match result {
            Err(err) => anyhow::bail!(err),
            Ok(aot::CreateChatCompletionStreamResponse { choices, .. }) => {
                for aot::ChatCompletionResponseStreamMessage {
                    delta:
                        aot::ChatCompletionStreamResponseDelta {
                            content,
                            function_call,
                            ..
                        },
                    finish_reason,
                    ..
                } in choices
                {
                    if let Some(content) = content {
                        stdout.write_all(content.as_ref()).await?;
                        stdout.flush().await?;
                        content_buffer.write_str(&content)?;
                    }
                    if let Some(aot::FunctionCallStream { name, arguments }) = function_call {
                        if let Some(name) = name {
                            function_call_name = name;
                        }
                        if let Some(arguments) = arguments {
                            function_call_arguments_buffer.write_str(&arguments)?;
                        }
                    }
                    if let Some(finish_reason) = finish_reason {
                        match finish_reason.as_ref() {
                            "stop" | "length" => {
                                stdout.write_all(b"\n").await?;
                                stdout.flush().await?;
                                stdout.shutdown().await?;
                                return Ok(aot::ChatCompletionRequestMessageArgs::default()
                                    .role(aot::Role::Assistant)
                                    .content(content_buffer.trim())
                                    .build()?);
                            }
                            "function_call" => {
                                let name = function_call_name.trim().into();
                                let arguments = function_call_arguments_buffer.trim().into();
                                return Ok(aot::ChatCompletionRequestMessageArgs::default()
                                    .role(aot::Role::Assistant)
                                    .function_call(aot::FunctionCall { name, arguments })
                                    .build()?);
                            }
                            // https://platform.openai.com/docs/api-reference/chat/streaming#choices-finish_reason
                            finish_reason => unreachable!("bad finish reason: {finish_reason}"),
                        }
                    }
                }
            }
        }
    }
    unreachable!("no finish reason")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let input = std::io::read_to_string(std::io::stdin().lock())?;
    let user_message = create_user_message(&input)?;
    let mut new_messages = vec![user_message];

    while !matches!(
        new_messages.iter().last().unwrap().role,
        aot::Role::Assistant
    ) {
        let messages = create_chat_messages(&new_messages);
        let request = create_request(messages)?;
        eprintln!("{request:#?}");
        let response = create_response(request).await?;
        let assistant_message = create_assistant_message(response).await?;

        update_new_messages(&mut new_messages, assistant_message)?;
    }

    // TODO: store new messages with atomicity guarantees: if
    // something fails,
    // nothing is stored,
    // so better store everything at the end.

    Ok(())
}

#[inline]
fn try_compact_json(s: impl Into<String>) -> String {
    let s = s.into();
    serde_json::from_str::<serde_json::Value>(&s)
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or(s)
}

#[inline]
fn create_function_call_message(
    name: &str,
    _arguments: &str,
) -> anyhow::Result<aot::ChatCompletionRequestMessage> {
    // TODO: eventually call functions,
    // see <https://github.com/64bit/async-openai/blob/37769355eae63d72b5d6498baa6c8cdcce910d71/examples/function-call-stream/src/main.rs#L67> and <https://github.com/64bit/async-openai/blob/37769355eae63d72b5d6498baa6c8cdcce910d71/examples/function-call-stream/src/main.rs#L84>

    let content = r#"{ "temperature": 22, "unit": "celsius", "description": "Sunny" }"#;
    let content = try_compact_json(content);
    Ok(aot::ChatCompletionRequestMessageArgs::default()
        .role(aot::Role::Function)
        .name(name)
        .content(content)
        .build()?)
}

#[inline]
fn update_new_messages(
    new_messages: &mut Vec<aot::ChatCompletionRequestMessage>,
    assistant_message: aot::ChatCompletionRequestMessage,
) -> anyhow::Result<()> {
    match assistant_message {
        aot::ChatCompletionRequestMessage {
            role: aot::Role::Assistant,
            content: Some(_),
            function_call: None,
            ..
        } => new_messages.push(assistant_message),
        aot::ChatCompletionRequestMessage {
            role: aot::Role::Assistant,
            content: None,
            function_call: Some(aot::FunctionCall { name, arguments }),
            ..
        } => {
            let arguments = try_compact_json(arguments);
            let function_call_message = create_function_call_message(&name, &arguments)?;

            new_messages.push(
                aot::ChatCompletionRequestMessageArgs::default()
                    .role(aot::Role::Assistant)
                    .function_call(aot::FunctionCall { name, arguments })
                    .build()?,
            );
            new_messages.push(function_call_message);
        }
        assistant_message => unreachable!("bad assistant message: {assistant_message:?}"),
    }

    Ok(())
}
