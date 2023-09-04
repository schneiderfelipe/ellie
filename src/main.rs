fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::io::read_to_string(std::io::stdin())?;
    let request = create_request(input)?;
    println!("{request:#?}");
    Ok(())
}

#[inline]
fn create_request(
    input: impl Into<String>,
) -> Result<async_openai::types::CreateChatCompletionRequest, async_openai::error::OpenAIError> {
    let temperature = 0.0;
    let messages = retrieve_messages(input)?;
    let model = "gpt-3.5-turbo";

    async_openai::types::CreateChatCompletionRequestArgs::default()
        .temperature(temperature)
        .messages(messages)
        .model(model)
        .build()
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
