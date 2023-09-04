use anyhow::Context;

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
    messages: &[async_openai::types::ChatCompletionRequestMessage],
) -> anyhow::Result<bool> {
    Ok(
        tiktoken_rs::async_openai::get_chat_completion_max_tokens(model, messages)?
            >= MIN_COMPLETION_TOKENS,
    )
}

/// Retrieve context messages for the given message.
#[inline]
fn retrieve_messages(
    message: async_openai::types::ChatCompletionRequestMessage,
) -> Vec<async_openai::types::ChatCompletionRequestMessage> {
    // TODO: actually get messages
    vec![message]
}

/// Find the cheapest model with large enough context length for the given
/// messages.
///
/// # Errors
/// If no model with large enough context length can be found.
#[inline]
fn choose_model(
    messages: &[async_openai::types::ChatCompletionRequestMessage],
) -> anyhow::Result<&'static str> {
    MODELS
        .into_iter()
        .find(|model| {
            messages_fit_model(model, messages)
                .expect("model retrieval of known models should never fail")
        })
        .context("no model with large enough context length could be found for the given messages")
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
) -> anyhow::Result<async_openai::types::ChatCompletionRequestMessage> {
    let messages = [
        async_openai::types::ChatCompletionRequestMessageArgs::default()
            .role(async_openai::types::Role::User)
            .content(input)
            .build()?,
    ];
    anyhow::ensure!(
        messages_fit_model(MODELS[0], &messages)?,
        "user input should fit the cheapest model alone"
    );
    let [message] = messages;
    Ok(message)
}

/// Create an `OpenAI` request.
///
/// # Errors
/// If context messages could not be retrieved or model could not be chosen for
/// the given message.
#[inline]
fn create_request(
    message: async_openai::types::ChatCompletionRequestMessage,
) -> anyhow::Result<async_openai::types::CreateChatCompletionRequest> {
    let messages = retrieve_messages(message);
    let model = choose_model(&messages)?;
    Ok(
        async_openai::types::CreateChatCompletionRequestArgs::default()
            .temperature(TEMPERATURE)
            .messages(messages)
            .model(model)
            .build()?,
    )
}

fn main() -> anyhow::Result<()> {
    let raw_input = read_input()?;
    let input = raw_input.trim();
    let message = create_user_message(input)?;
    let request = create_request(message)?;
    println!("{request:#?}");
    Ok(())
}
