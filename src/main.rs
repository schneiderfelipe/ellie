use std::io::Write;

use anyhow::Context;
use async_openai::types as aot;
use futures::StreamExt;

/// Temperature used in all requests.
const TEMPERATURE: f32 = 0.0;

/// Minimum number of tokens to be able to generate in the completion.
const MIN_COMPLETION_TOKENS: usize = 256;

/// Available `OpenAI` models sorted by their prices.
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
/// If the model can not be retrieved.
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
/// If no model with large enough context length can be found, this returns
/// [`None`].
#[inline]
fn choose_model(messages: &[aot::ChatCompletionRequestMessage]) -> Option<&'static str> {
    MODELS.into_iter().find(|model| {
        messages_fit_model(model, messages)
            .expect("model retrieval of known models should never fail")
    })
}

/// Read the whole standard input.
///
/// # Errors
/// If the input contains invalid UTF-8 or any other situation where
/// [`std::io::read_to_string`] fails.
#[inline]
fn read_input() -> std::io::Result<String> {
    std::io::read_to_string(std::io::stdin().lock())
}

/// Create a user message for the given input.
///
/// # Errors
/// If the created message could not fit the cheapest model alone.
#[inline]
fn create_user_message(
    input: impl Into<String>,
) -> anyhow::Result<aot::ChatCompletionRequestMessage> {
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

/// Retrieve chat messages for the given message.
#[inline]
fn create_chat_messages(
    message: aot::ChatCompletionRequestMessage,
) -> Vec<aot::ChatCompletionRequestMessage> {
    // TODO: actually get messages
    vec![message]
}

/// Create an `OpenAI` request.
///
/// # Errors
/// If chat messages could not be retrieved or model could not be chosen for
/// the given message.
#[inline]
fn create_request(
    messages: Vec<aot::ChatCompletionRequestMessage>,
) -> anyhow::Result<aot::CreateChatCompletionRequest> {
    let model = choose_model(&messages).context(
        "no model with large enough context length could be found for the given messages",
    )?;
    Ok(aot::CreateChatCompletionRequestArgs::default()
        .temperature(TEMPERATURE)
        .messages(messages)
        .model(model)
        .build()?)
}

#[inline]
async fn create_stream(
    request: aot::CreateChatCompletionRequest,
) -> Result<aot::ChatCompletionResponseStream, async_openai::error::OpenAIError> {
    async_openai::Client::new()
        .chat()
        .create_stream(request)
        .await
}

#[inline]
async fn create_response(mut stream: aot::ChatCompletionResponseStream) -> anyhow::Result<String> {
    let mut stdout = std::io::stdout().lock();
    while let Some(result) = stream.next().await {
        match result {
            Err(err) => anyhow::bail!(err),
            Ok(aot::CreateChatCompletionStreamResponse { choices, .. }) => {
                choices.into_iter().try_for_each(
                    |aot::ChatCompletionResponseStreamMessage {
                         delta: aot::ChatCompletionStreamResponseDelta { content, .. },
                         ..
                     }| match content {
                        None => Ok(()),
                        Some(ref content) => {
                            write!(stdout, "{content}")?;
                            stdout.flush()
                        }
                    },
                )?
            }
        }
    }
    writeln!(stdout)?;
    // TODO: create user/assistant pair, trim response and store
    Ok(String::new())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let raw_input = read_input()?;
    let input = raw_input.trim();
    let message = create_user_message(input)?;
    let messages = create_chat_messages(message);
    let request = create_request(messages)?;
    let stream = create_stream(request).await?;
    let _response = create_response(stream).await?;
    Ok(())
}
