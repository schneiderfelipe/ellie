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
    provider: Vec<Provider>,
    function: Vec<aot::ChatCompletionFunctions>,
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
        // TODO: the logic here changed,
        // things should be calculated now
        functions.function
    }
}

impl Functions {
    #[inline]
    pub(super) fn load() -> eyre::Result<Self> {
        // TODO: actually get a path to the user config file.
        let content = std::fs::read_to_string("functions.toml")?;
        let Self { provider, function } = toml::from_str(&content)?;

        // TODO: check names in functions and providers are all distinct.

        // TODO: make sure there is a provider for each function,
        // i.e.,
        // provider.len() >= function.len()
        // TODO: alternatively,
        // simply ignore functions without providers and give a warning.

        // TODO: calculate specifications on demand
        // for provider in &provider {
        //     let specification = unimplemented!();
        //     function.push(specification)
        // }

        // TODO: here's the place to validate stuff.

        Ok(Self { provider, function })
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
    fn get_provider(&self, name: &str) -> Option<&Provider> {
        self.provider.iter().find(|provider| provider.name == name)
    }

    #[inline]
    fn get_function(&self, name: &str) -> Option<&aot::ChatCompletionFunctions> {
        self.function.iter().find(|function| function.name == name)
    }

    #[inline]
    pub(super) fn call(&self, name: &str, arguments: &str) -> eyre::Result<String> {
        self.get_provider(name)
            .context("getting function provider")?
            .call(arguments)
    }

    #[inline]
    fn specification(&self, name: &str) -> eyre::Result<aot::ChatCompletionFunctions> {
        let mut specification = self
            .get_provider(name)
            .context("getting function provider")?
            .specification()?;

        if let Some(aot::ChatCompletionFunctions {
            name: _,
            description,
            parameters,
        }) = self.get_function(&specification.name)
        {
            specification.description = description.clone().or(specification.description);
            if let (Some(specification_parameters), Some(parameters)) =
                (&mut specification.parameters, &parameters)
            {
                json_patch::merge(specification_parameters, parameters);
            } else {
                specification.parameters = parameters.clone().or(specification.parameters);
            }
        }

        Ok(specification)
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
        // TODO: in the future we might accept multiple json objects and prune based on
        // messages.
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
