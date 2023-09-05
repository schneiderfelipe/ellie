use async_openai::types as aot;

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
    // TODO: actually get messages, and split/prune to fit the context, this should
    // always produce a valid context for at least *one* of the MODELS.
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
    use anyhow::Context as _;

    let model = choose_model(&messages).context(
        "no model with large enough context length could be found for the given messages",
    )?;
    Ok(aot::CreateChatCompletionRequestArgs::default()
        .temperature(TEMPERATURE)
        .messages(messages)
        .model(model)
        // TODO: we could add the user name here
        // TODO: function specifications will be added in the future here
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
async fn handle_response(
    mut response: aot::ChatCompletionResponseStream,
) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    use futures::StreamExt as _;
    use tokio::io::AsyncWriteExt as _;

    let mut stdout = tokio::io::stdout();
    let mut buffer = String::new();
    'outer: while let Some(result) = response.next().await {
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
                        stdout.write_all(content.as_bytes()).await?;
                        stdout.flush().await?;
                        buffer.write_str(&content)?;
                    }
                    if let Some(aot::FunctionCallStream {
                        name: _,
                        arguments: _,
                    }) = function_call
                    {
                        // TODO: we might have a function call here, see <https://community.openai.com/t/function-calls-and-streaming/263393/3?u=schneider.felipe.5> for how to proceed. This will change our return type to an enum.
                        unimplemented!()
                    }
                    if let Some(finish_reason) = finish_reason {
                        // TODO: we might want to return or break from here, see <https://community.openai.com/t/function-calls-and-streaming/263393/3?u=schneider.felipe.5> for how to proceed.
                        match finish_reason.as_str() {
                            "stop" => break 'outer,
                            "length" => unimplemented!(),
                            "function_call" => unimplemented!(),
                            finish_reason => {
                                // https://platform.openai.com/docs/api-reference/chat/streaming#choices-finish_reason
                                unreachable!("impossible finish reason: {finish_reason}")
                            }
                        }
                    }
                }
            }
        }
    }
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;
    stdout.shutdown().await?;
    Ok(buffer)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let raw_input = std::io::read_to_string(std::io::stdin().lock())?;
    let input = raw_input.trim();
    let message = create_user_message(input)?;

    let messages = create_chat_messages(message);
    let request = create_request(messages)?;
    let response = create_response(request).await?;
    let _output = handle_response(response).await?;

    // println!("{output}");
    // TODO: create user/assistant pair, trim output and store
    Ok(())
}
