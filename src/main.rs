use async_openai::types as aot;
use color_eyre::eyre;
use eyre::{Context as _, ContextCompat as _};

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
) -> eyre::Result<bool> {
    Ok(
        tiktoken_rs::async_openai::get_chat_completion_max_tokens(model, messages)
            .map_err(|e| eyre::eyre!(e))?
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

#[inline]
fn try_compact_json(maybe_json: &str) -> String {
    let maybe_json = maybe_json.trim();
    serde_json::from_str::<serde_json::Value>(maybe_json)
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or_else(|_| maybe_json.into())
}

#[inline]
fn create_function_call(name: &str, arguments: &str) -> aot::FunctionCall {
    let name = name.trim();
    aot::FunctionCall {
        name: name.into(),
        arguments: try_compact_json(arguments),
    }
}

#[inline]
fn create_function_call_message(
    name: &str,
    _arguments: &str,
) -> eyre::Result<aot::ChatCompletionRequestMessage> {
    // TODO: eventually call functions,
    // see <https://github.com/64bit/async-openai/blob/37769355eae63d72b5d6498baa6c8cdcce910d71/examples/function-call-stream/src/main.rs#L67> and <https://github.com/64bit/async-openai/blob/37769355eae63d72b5d6498baa6c8cdcce910d71/examples/function-call-stream/src/main.rs#L84>

    let content = r#"{"location": "Boston, MA", "temperature": "72", "unit": null, "forecast": ["sunny", "windy"]}"#;
    Ok(aot::ChatCompletionRequestMessageArgs::default()
        .role(aot::Role::Function)
        .name(name)
        .content(try_compact_json(content))
        .build()?)
}

/// Create a user message for the given input.
///
/// # Errors
/// If the created message could not fit the cheapest model alone.
#[inline]
fn create_user_message(input: &str) -> eyre::Result<aot::ChatCompletionRequestMessage> {
    let input = input.trim();
    let messages = [aot::ChatCompletionRequestMessageArgs::default()
        .role(aot::Role::User)
        .content(input)
        .build()?];
    eyre::ensure!(
        messages_fit_model(MODELS[0], &messages)?,
        "user input should fit {model}",
        model = MODELS[0]
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
) -> eyre::Result<aot::CreateChatCompletionRequest> {
    let model = choose_model(&messages)
        .context("choosing model with large enough context length for the given messages")?;
    Ok(aot::CreateChatCompletionRequestArgs::default()
        .temperature(TEMPERATURE)
        .messages(messages)
        .model(model)
        // TODO: actual function specifications will be added in the future here
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
async fn create_response<C: async_openai::config::Config + Sync>(
    client: &async_openai::Client<C>,
    request: aot::CreateChatCompletionRequest,
) -> Result<aot::ChatCompletionResponseStream, async_openai::error::OpenAIError> {
    log::info!(
        "{request}",
        request =
            serde_json::to_string(&request).expect("serialization of requests should never fail")
    );
    client.chat().create_stream(request).await
}

#[inline]
async fn create_assistant_message(
    mut response: aot::ChatCompletionResponseStream,
) -> eyre::Result<aot::ChatCompletionRequestMessage> {
    use std::fmt::Write as _;

    use futures::StreamExt as _;
    use tokio::io::AsyncWriteExt as _;

    let mut stdout = tokio::io::stdout();
    let mut content_buffer = String::new();
    let mut function_call_name = String::new();
    let mut function_call_arguments_buffer = String::new();
    while let Some(result) = response.next().await {
        match result.context("receiving response chunk") {
            Err(err) => eyre::bail!(err),
            Ok(aot::CreateChatCompletionStreamResponse { choices, .. }) => {
                for aot::ChatCompletionResponseStreamMessage {
                    delta:
                        aot::ChatCompletionStreamResponseDelta {
                            role,
                            content,
                            function_call,
                        },
                    finish_reason,
                    ..
                } in choices
                {
                    if let Some(role) = role {
                        eyre::ensure!(matches!(role, aot::Role::Assistant), "bad role: {role}");
                    }
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
                                return Ok(aot::ChatCompletionRequestMessageArgs::default()
                                    .role(aot::Role::Assistant)
                                    .content("") // BUG: https://github.com/64bit/async-openai/issues/103#issue-1884273236
                                    .function_call(create_function_call(&function_call_name, &function_call_arguments_buffer))
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

#[inline]
fn update_new_messages(
    new_messages: &mut Vec<aot::ChatCompletionRequestMessage>,
    assistant_message: aot::ChatCompletionRequestMessage,
) -> eyre::Result<()> {
    match assistant_message {
        aot::ChatCompletionRequestMessage {
            role: aot::Role::Assistant,
            name: None,
            content: Some(_),
            function_call: None,
        } => new_messages.push(assistant_message),
        aot::ChatCompletionRequestMessage {
            role: aot::Role::Assistant,
            name: None,
            ref content,
            function_call:
                Some(aot::FunctionCall {
                    ref name,
                    ref arguments,
                }),
        } if content.is_none()
            || content
                .as_ref()
                // BUG: https://github.com/64bit/async-openai/issues/103#issue-1884273236
                .is_some_and(|content| content.trim().is_empty()) =>
        {
            let function_call_message = create_function_call_message(name, arguments)?;
            new_messages.push(assistant_message);
            new_messages.push(function_call_message);
        }
        assistant_message => unreachable!("bad assistant message: {assistant_message:?}"),
    }

    Ok(())
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    pretty_env_logger::init();
    color_eyre::install()?;

    let input = std::io::read_to_string(std::io::stdin().lock())?;
    let user_message = create_user_message(&input)?;
    let mut new_messages = vec![user_message];

    let client = async_openai::Client::new();
    while !matches!(
        new_messages
            .iter()
            .last()
            .expect("there should always be at least one new message")
            .role,
        aot::Role::Assistant
    ) {
        let messages = create_chat_messages(&new_messages);
        let request = create_request(messages)?;
        let response = create_response(&client, request).await?;
        let assistant_message = create_assistant_message(response)
            .await
            .context("creating assistant message")?;

        update_new_messages(&mut new_messages, assistant_message)?;
    }

    // TODO: store new messages with atomicity guarantees: if
    // something fails,
    // nothing is stored,
    // so better store everything at the end.

    Ok(())
}
