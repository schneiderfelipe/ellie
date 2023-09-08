use async_openai::types as aot;
use color_eyre::eyre;
use eyre::ContextCompat as _;

/// Trim text
/// and try to produce a compact JSON string out of it,
/// returning an owned trimmed string if serialization fails.
#[inline]
pub fn try_compact_json(maybe_json: &str) -> String {
    let maybe_json = maybe_json.trim();
    serde_json::from_str::<serde_json::Value>(maybe_json)
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or_else(|_| maybe_json.into())
}

#[derive(Debug, serde::Deserialize)]
pub struct Functions {
    function: Vec<aot::ChatCompletionFunctions>,
    provider: Vec<Provider>,
}
#[derive(Debug, serde::Deserialize)]
struct Provider {
    name: String,
    command: String,
    args: Vec<String>,
}

impl From<Functions> for Vec<aot::ChatCompletionFunctions> {
    #[inline]
    fn from(functions: Functions) -> Self {
        functions.function
    }
}

impl Functions {
    #[inline]
    pub(super) fn load() -> eyre::Result<Self> {
        // TODO: actually get a path to the user config file.
        let functions = std::fs::read_to_string("functions.toml")?;
        let functions = toml::from_str(&functions)?;

        // TODO: actual function specifications will be optionally retrieved directly
        // from binaries/scripts in the future.
        // There will be a way of overriding
        // what the binary/script says using the configuration file,
        // so that the
        // configuration file is mostly clean most of the time.

        // TODO: make sure there is a provider for each function,
        // i.e.,
        // provider.len() >= function.len()
        // TODO: alternatively,
        // simply ignore functions without providers and give a warning.

        Ok(functions)
    }

    #[inline]
    pub(super) fn prune(
        self,
        _messages: &[aot::ChatCompletionRequestMessage],
    ) -> eyre::Result<Self> {
        // TODO: actually choose relevant functions based on the chat messages.
        Ok(self)
    }

    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.provider.is_empty()
    }

    #[inline]
    pub(super) fn call(&self, name: &str, arguments: &str) -> eyre::Result<String> {
        let provider = self
            .provider
            .iter()
            .find(|provider| provider.name == name)
            .context("getting function provider")?;

        // TODO: see <https://github.com/64bit/async-openai/blob/37769355eae63d72b5d6498baa6c8cdcce910d71/examples/function-call-stream/src/main.rs#L67>
        // and <https://github.com/64bit/async-openai/blob/37769355eae63d72b5d6498baa6c8cdcce910d71/examples/function-call-stream/src/main.rs#L84>.
        let content = duct::cmd(&provider.command, &provider.args)
            .stdin_bytes(arguments)
            .read()?;

        Ok(try_compact_json(&content))
    }
}
