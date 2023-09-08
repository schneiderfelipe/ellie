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
        let content = std::fs::read_to_string("functions.toml")?;
        let Self {
            mut function,
            provider,
        } = toml::from_str(&content)?;

        // TODO: check names in functions and providers are all distinct.

        // TODO: make sure there is a provider for each function,
        // i.e.,
        // provider.len() >= function.len()
        // TODO: alternatively,
        // simply ignore functions without providers and give a warning.

        // TODO: calculate specifications on demand
        for provider in &provider {
            let aot::ChatCompletionFunctions {
                name: actual_name,
                description: mut actual_description,
                parameters: mut actual_parameters,
            } = provider.specification()?;

            if let Some(index) = function
                .iter()
                .position(|function| function.name == actual_name)
            {
                let aot::ChatCompletionFunctions {
                    name: _,
                    description,
                    parameters,
                } = function.swap_remove(index);
                actual_description = description.or(actual_description);

                if let (Some(actual_parameters), Some(parameters)) =
                    (&mut actual_parameters, &parameters)
                {
                    json_patch::merge(actual_parameters, parameters);
                } else {
                    actual_parameters = parameters.or(actual_parameters);
                }
            }

            let mut specification = aot::ChatCompletionFunctionsArgs::default();
            specification.name(actual_name);
            if let Some(actual_description) = actual_description {
                specification.description(actual_description);
            }
            if let Some(actual_parameters) = actual_parameters {
                specification.parameters(actual_parameters);
            }

            function.push(specification.build()?)
        }

        Ok(Self { function, provider })
    }

    #[inline]
    pub(super) fn prune(
        self,
        _messages: &[aot::ChatCompletionRequestMessage],
    ) -> eyre::Result<Self> {
        // TODO: actually choose relevant functions based on the chat messages.
        // This might become a separate function on specifications alone,
        // who knows.
        Ok(self)
    }

    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.provider.is_empty()
    }

    #[inline]
    pub(super) fn call(&self, name: &str, arguments: &str) -> eyre::Result<String> {
        self.provider
            .iter()
            .find(|provider| provider.name == name)
            .context("getting function provider")?
            .call(arguments)
    }
}

impl Provider {
    #[inline]
    fn call(&self, arguments: &str) -> eyre::Result<String> {
        // TODO: see <https://github.com/64bit/async-openai/blob/37769355eae63d72b5d6498baa6c8cdcce910d71/examples/function-call-stream/src/main.rs#L67>
        // and <https://github.com/64bit/async-openai/blob/37769355eae63d72b5d6498baa6c8cdcce910d71/examples/function-call-stream/src/main.rs#L84>.
        let content = duct::cmd(&self.command, &self.args)
            .stdin_bytes(arguments)
            .read()?;
        Ok(try_compact_json(&content))
    }

    #[inline]
    fn specification(&self) -> eyre::Result<aot::ChatCompletionFunctions> {
        let specification = duct::cmd(
            &self.command,
            self.args
                .iter()
                .map(AsRef::as_ref)
                .chain(std::iter::once("specification")),
        )
        .read()?;

        let mut specification: aot::ChatCompletionFunctions = serde_json::from_str(&specification)?;
        if specification.name != self.name {
            log::warn!("{} != {}", self.name, specification.name);
            specification.name = self.name.clone();
        }

        Ok(specification)
    }
}
