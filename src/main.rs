fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::io::read_to_string(std::io::stdin())?;
    let request = create_request(input)?;
    println!("{request:#?}");
    Ok(())
}

#[inline]
fn create_request(
    input: String,
) -> Result<async_openai::types::CreateChatCompletionRequest, async_openai::error::OpenAIError> {
    let model = "gpt-3.5-turbo";
    let max_tokens = 512u16;
    let messages = [
        async_openai::types::ChatCompletionRequestMessageArgs::default()
            .content(input)
            .role(async_openai::types::Role::User)
            .build()?,
    ];

    async_openai::types::CreateChatCompletionRequestArgs::default()
        .model(model)
        .max_tokens(max_tokens)
        .messages(messages)
        .build()
}
