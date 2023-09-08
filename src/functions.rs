use async_openai::types::ChatCompletionFunctions;
use color_eyre::eyre;

/// Trim text
/// and try to produce a compact JSON string out of it,
/// returning an owned trimmed string if serialization fails.
#[inline]
pub fn try_compact_json(maybe_json: &str) -> String {
    let maybe_json = maybe_json.trim();
    serde_json::from_str::<serde_json::Value>(maybe_json)
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or_else(|_| maybe_json.to_owned())
}

#[inline]
fn merge(specification: &mut ChatCompletionFunctions, patch: &ChatCompletionFunctions) {
    let ChatCompletionFunctions {
        name: _,
        description,
        parameters,
    } = patch;

    if let Some(description) = description {
        specification.description = Some(description.clone());
    }
    if let (Some(specification_parameters), Some(parameters)) =
        (&mut specification.parameters, &parameters)
    {
        json_patch::merge(specification_parameters, parameters);
    } else if let Some(parameters) = parameters {
        specification.parameters = Some(parameters.clone());
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct Provider {
    name: String,
    command: String,
    args: Vec<String>,
}

impl Provider {
    #[inline]
    pub(super) fn call(&self, arguments: &str) -> std::io::Result<String> {
        // TODO: see <https://github.com/64bit/async-openai/blob/37769355eae63d72b5d6498baa6c8cdcce910d71/examples/function-call-stream/src/main.rs#L67>
        // and <https://github.com/64bit/async-openai/blob/37769355eae63d72b5d6498baa6c8cdcce910d71/examples/function-call-stream/src/main.rs#L84>.
        let content = duct::cmd(&self.command, &self.args)
            .stdin_bytes(arguments)
            .read()?;
        // TODO: in the future we might accept multiple json objects and select based on
        // messages.
        Ok(try_compact_json(&content))
    }

    #[inline]
    fn specification(&self) -> eyre::Result<ChatCompletionFunctions> {
        let specification = duct::cmd(
            &self.command,
            self.args
                .iter()
                .map(AsRef::as_ref)
                .chain(std::iter::once("specification")),
        )
        .read()?;

        let mut specification: ChatCompletionFunctions = serde_json::from_str(&specification)?;
        if specification.name != self.name {
            log::warn!("{} != {}", self.name, specification.name);
            specification.name = self.name.clone();
        }

        Ok(specification)
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct Functions {
    provider: Vec<Provider>,
    function: Vec<ChatCompletionFunctions>,
}

impl Functions {
    #[inline]
    pub(super) fn load() -> eyre::Result<Self> {
        // TODO: actually get a path to the user config file.
        let content = std::fs::read_to_string("functions.toml")?;
        let functions = toml::from_str(&content)?;

        // TODO: here's the place to validate stuff.
        // TODO: check names in functions and providers are all distinct (unique,
        // the first found is the unique one).
        // TODO: simply ignore functions without providers,
        // but give a warning each time.

        Ok(functions)
    }

    #[inline]
    fn providers(&self) -> impl Iterator<Item = &Provider> {
        self.provider.iter()
    }

    #[inline]
    fn functions(&self) -> impl Iterator<Item = &ChatCompletionFunctions> {
        self.function.iter()
    }

    #[inline]
    pub(super) fn get_provider(&self, name: &str) -> Option<&Provider> {
        self.providers().find(|provider| provider.name == name)
    }

    #[inline]
    fn get_function(&self, name: &str) -> Option<&ChatCompletionFunctions> {
        self.functions().find(|function| function.name == name)
    }

    #[inline]
    pub(super) fn specifications(
        &self,
    ) -> impl Iterator<Item = eyre::Result<ChatCompletionFunctions>> + '_ {
        self.providers().map(|provider| {
            let mut specification = provider.specification()?;
            if let Some(function) = self.get_function(&specification.name) {
                merge(&mut specification, function);
            }
            Ok(specification)
        })
    }
}
