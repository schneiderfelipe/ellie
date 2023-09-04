/// Available `OpenAI` models sorted by their prices.
const MODELS: [&str; 4] = [
    "gpt-3.5-turbo",     // $0.0015 / 1K tokens
    "gpt-3.5-turbo-16k", // $0.003  / 1K tokens
    "gpt-4",             // $0.03   / 1K tokens
    "gpt-4-32k",         // $0.06   / 1K tokens
];

/// Get the cheapest model fitting the given chat context.
#[inline]
fn choose_model(
    messages: &[async_openai::types::ChatCompletionRequestMessage],
) -> anyhow::Result<Option<impl Into<String>>> {
    let model = MODELS.into_iter().find(|model| {
        tiktoken_rs::async_openai::get_chat_completion_max_tokens(model, messages)
            .expect("model retrieval should not fail")
            > 0
    });

    Ok(model)
}

#[inline]
fn create_request(
    input: impl Into<String>,
) -> anyhow::Result<async_openai::types::CreateChatCompletionRequest> {
    let temperature = 0.0;
    let messages = retrieve_messages(input)?.into();
    let model = choose_model(&messages)?.expect("a model should be available");

    let request = async_openai::types::CreateChatCompletionRequestArgs::default()
        .temperature(temperature)
        .messages(messages)
        .model(model)
        .build()?;

    Ok(request)
}

#[inline]
fn retrieve_messages(
    input: impl Into<String>,
) -> Result<
    impl Into<Vec<async_openai::types::ChatCompletionRequestMessage>>,
    async_openai::error::OpenAIError,
> {
    let messages = [
        async_openai::types::ChatCompletionRequestMessageArgs::default()
            .role(async_openai::types::Role::User)
            .content(input)
            .build()?,
    ];

    Ok(messages)
}

fn main() -> anyhow::Result<()> {
    let input = std::io::read_to_string(std::io::stdin())?;
    let request = create_request(input)?;
    println!("{request:#?}");
    Ok(())
}
