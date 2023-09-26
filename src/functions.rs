use async_openai::types::ChatCompletionFunctions;

#[inline]
fn get_project_dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("io.github", "schneiderfelipe", "ellie")
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

    /// Command-line arguments to pass to command execution.
    #[serde(default)]
    args: Vec<String>,

    /// Whether this provider command can be safely executed *without user
    /// approval*.
    #[serde(default)]
    safe: bool,
}

impl Provider {
    #[inline]
    fn is_approved(&self, arguments: &str) -> dialoguer::Result<bool> {
        log::warn!("{name}({arguments})", name = self.name);
        let is_approved = self.safe
            || dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt("Do you approve command execution?")
                .interact()?;
        Ok(is_approved)
    }

    /// Call provider with the given standard input arguments,
    /// returning the output produced by command execution.
    ///
    /// If denied by the user,
    /// command execution is aborted,
    /// and this function returns [`None`].
    #[inline]
    fn call(&self, arguments: &str) -> dialoguer::Result<Option<String>> {
        let response = if self.is_approved(arguments)? {
            let response = duct::cmd(&self.command, &self.args)
                .stdin_bytes(arguments)
                .stderr_to_stdout()
                .unchecked()
                .read()
                .expect("unchecked command execution should never fail");
            Some(response)
        } else {
            None
        };
        Ok(response)
    }

    #[inline]
    fn specification(
        &self,
    ) -> Result<ChatCompletionFunctions, either::Either<serde_json::Error, std::io::Error>> {
        let spec = duct::cmd(
            &self.command,
            self.args
                .iter()
                .map(AsRef::as_ref)
                .chain(std::iter::once("spec")),
        )
        .read()
        .map_err(either::Either::Right)?;

        let mut spec: ChatCompletionFunctions =
            serde_json::from_str(&spec).map_err(either::Either::Left)?;
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
        use color_eyre::eyre::ContextCompat as _;
        use itertools::Itertools as _;

        let content = std::fs::read_to_string(
            get_project_dirs()
                .context("getting project directories")?
                .config_dir()
                .join("functions.toml"),
        )?;
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
    pub(super) fn call(&self, name: &str, arguments: &str) -> dialoguer::Result<FunctionResponse> {
        let response = if let Some(provider) = self.get_provider(name) {
            provider
                .call(arguments)?
                .map_or(FunctionResponse::Aborted, FunctionResponse::Executed)
        } else {
            FunctionResponse::NotFound
        };
        Ok(response)
    }

    #[inline]
    pub(super) fn specifications(
        &self,
    ) -> impl Iterator<Item = color_eyre::eyre::Result<ChatCompletionFunctions>> + '_ {
        use color_eyre::eyre::Context as _;

        self.providers().map(|provider| {
            let mut spec = provider
                .specification()
                .with_context(|| format!("getting function specification for {provider:?}"))?;
            if let Some(function) = self.get_function(&spec.name) {
                merge(&mut spec, function);
            }
            Ok(spec)
        })
    }
}

#[derive(Debug)]
pub enum FunctionResponse {
    Executed(String),
    Aborted,
    NotFound,
}

impl std::fmt::Display for FunctionResponse {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Executed(output) => write!(f, "{output}", output = try_compact_json(output)),
            Self::Aborted => write!(f, "function call aborted: user denied command execution"),
            Self::NotFound => write!(
                f,
                "function not found: the requested function is currently unavailable or not \
                 implemented yet"
            ),
        }
    }
}
