use async_openai::types::ChatCompletionFunctions;

#[inline]
fn get_project_dirs() -> color_eyre::eyre::Result<directories::ProjectDirs> {
    use color_eyre::eyre::ContextCompat as _;

    directories::ProjectDirs::from("io.github", "schneiderfelipe", "ellie")
        .context("getting project directories")
}

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
fn merge(spec: &mut ChatCompletionFunctions, patch: &ChatCompletionFunctions) {
    let ChatCompletionFunctions {
        name: _,
        description,
        parameters,
    } = patch;

    if let Some(description) = description {
        spec.description = Some(description.clone());
    }
    if let (Some(spec_parameters), Some(parameters)) = (&mut spec.parameters, &parameters) {
        json_patch::merge(spec_parameters, parameters);
    } else if let Some(parameters) = parameters {
        spec.parameters = Some(parameters.clone());
    }
}

/// Function provider.
#[derive(Debug, serde::Deserialize)]
struct Provider {
    /// Function provider name.
    name: String,

    /// Command to execute.
    command: String,

    /// Command-line arguments to pass to command.
    #[serde(default)]
    args: Vec<String>,

    /// Whether this provider can be safely executed without user confirmation.
    #[serde(default)]
    safe: bool,
}

impl Provider {
    /// Call provider with the given standard input arguments.
    #[inline]
    fn call(&self, arguments: &str) -> Result<ProviderResponse, dialoguer::Error> {
        log::warn!("{name}({arguments})", name = self.name);
        let response = if self.safe
            || dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt("Do you want to execute?")
                .interact()?
        {
            duct::cmd(&self.command, &self.args)
                .stdin_bytes(arguments)
                .stderr_to_stdout()
                .unchecked()
                .read()
                .map(ProviderResponse::Executed)
                .expect("unchecked commands should never fail")
        } else {
            ProviderResponse::Aborted
        };
        Ok(response)
    }

    #[inline]
    fn specification(&self) -> color_eyre::eyre::Result<ChatCompletionFunctions> {
        use color_eyre::eyre::Context as _;

        let spec = duct::cmd(
            &self.command,
            self.args
                .iter()
                .map(AsRef::as_ref)
                .chain(std::iter::once("spec")),
        )
        .read()
        .with_context(|| {
            format!(
                "getting function specification for '{name}'",
                name = self.name
            )
        })?;

        let mut spec: ChatCompletionFunctions = serde_json::from_str(&spec)?;
        if spec.name != self.name {
            log::warn!("'{name}' != '{other}'", name = self.name, other = spec.name);
            spec.name = self.name.clone();
        }

        Ok(spec)
    }
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct Functions {
    #[serde(default)]
    provider: Vec<Provider>,
    #[serde(default)]
    function: Vec<ChatCompletionFunctions>,
}

impl Functions {
    #[inline]
    pub(super) fn load() -> color_eyre::eyre::Result<Self> {
        use itertools::Itertools as _;

        let content =
            std::fs::read_to_string(get_project_dirs()?.config_dir().join("functions.toml"))?;
        let Self { provider, function } = toml::from_str(&content)?;

        let provider: Vec<_> = provider
            .into_iter()
            .sorted_by(|p, q| p.name.cmp(&q.name))
            .dedup_by_with_count(|p, q| p.name == q.name)
            .inspect(|(count, provider)| {
                if *count > 1 {
                    log::warn!(
                        "provider '{name}' defined {count} times",
                        name = provider.name
                    );
                }
            })
            .map(|(_, provider)| provider)
            .map(
                |Provider {
                     name,
                     command,
                     args,
                     safe,
                 }| {
                    args.into_iter()
                        .map(|arg| shellexpand::full(&arg).map(Into::into))
                        .collect::<Result<_, _>>()
                        .map(|args| Provider {
                            name,
                            command,
                            args,
                            safe,
                        })
                },
            )
            .collect::<Result<_, _>>()?;
        let function = function
            .into_iter()
            .sorted_by(|f, g| f.name.cmp(&g.name))
            .dedup_by_with_count(|f, g| f.name == g.name)
            .inspect(|(count, function)| {
                if *count > 1 {
                    log::warn!(
                        "function '{name}' defined {count} times",
                        name = function.name
                    );
                }
                if !provider
                    .iter()
                    .any(|provider| provider.name == function.name)
                {
                    log::warn!("function '{name}' has no provider", name = function.name);
                }
            })
            .map(|(_, function)| function)
            .collect();

        Ok(Self { provider, function })
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
    fn get_provider(&self, name: &str) -> Option<&Provider> {
        self.providers().find(|provider| provider.name == name)
    }

    #[inline]
    fn get_function(&self, name: &str) -> Option<&ChatCompletionFunctions> {
        self.functions().find(|function| function.name == name)
    }

    #[inline]
    pub(super) fn call(
        &self,
        name: &str,
        arguments: &str,
    ) -> Result<FunctionResponse, dialoguer::Error> {
        self.get_provider(name).map_or_else(
            || Ok(FunctionResponse::NotFound),
            |provider| provider.call(arguments).map(Into::into),
        )
    }

    #[inline]
    pub(super) fn specifications(
        &self,
    ) -> impl Iterator<Item = color_eyre::eyre::Result<ChatCompletionFunctions>> + '_ {
        self.providers().map(|provider| {
            let mut spec = provider.specification()?;
            if let Some(function) = self.get_function(&spec.name) {
                merge(&mut spec, function);
            }
            Ok(spec)
        })
    }
}

pub enum FunctionResponse {
    Found(ProviderResponse),
    NotFound,
}

pub enum ProviderResponse {
    Executed(String),
    Aborted,
}

impl std::fmt::Display for FunctionResponse {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Found(response) => write!(f, "{response}"),
            Self::NotFound => write!(f, "function not found or not implemented"),
        }
    }
}

impl std::fmt::Display for ProviderResponse {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Executed(output) => write!(f, "{output}", output = try_compact_json(output)),
            Self::Aborted => write!(f, "function call aborted by the user"),
        }
    }
}

impl From<FunctionResponse> for String {
    #[inline]
    fn from(response: FunctionResponse) -> Self {
        response.to_string()
    }
}

impl From<ProviderResponse> for FunctionResponse {
    #[inline]
    fn from(response: ProviderResponse) -> Self {
        Self::Found(response)
    }
}
