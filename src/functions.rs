use async_openai::types::ChatCompletionFunctions;
use color_eyre::eyre;

#[inline]
fn get_project_dirs() -> color_eyre::Result<directories::ProjectDirs> {
    use eyre::ContextCompat as _;

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

#[derive(Debug, serde::Deserialize)]
pub struct Provider {
    name: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
}

impl Provider {
    #[inline]
    pub(super) fn call(&self, arguments: &str) -> color_eyre::Result<String> {
        use eyre::Context as _;

        let content = duct::cmd(&self.command, &self.args)
            .stdin_bytes(arguments)
            .read()
            .context("calling function")?;
        Ok(try_compact_json(&content))
    }

    #[inline]
    fn specification(&self) -> eyre::Result<ChatCompletionFunctions> {
        use eyre::Context as _;

        let spec = duct::cmd(
            &self.command,
            self.args
                .iter()
                .map(AsRef::as_ref)
                .chain(std::iter::once("spec")),
        )
        .read()
        .context("getting function specification")?;

        let mut spec: ChatCompletionFunctions = serde_json::from_str(&spec)?;
        if spec.name != self.name {
            log::warn!("{} != {}", self.name, spec.name);
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
    pub(super) fn load() -> eyre::Result<Self> {
        use itertools::Itertools as _;

        let content =
            std::fs::read_to_string(get_project_dirs()?.config_dir().join("functions.toml"))?;
        let Self { provider, function } = toml::from_str(&content)?;

        let provider = <Result<Vec<_>, _>>::from_iter(
            provider
                .into_iter()
                .sorted_by(|p, q| p.name.cmp(&q.name))
                .dedup_by_with_count(|p, q| p.name == q.name)
                .inspect(|(count, provider)| {
                    if *count > 1 {
                        log::warn!("provider {} defined {} times", provider.name, count);
                    }
                })
                .map(|(_, provider)| provider)
                .map(
                    |Provider {
                         name,
                         command,
                         args,
                     }|
                     -> Result<_, shellexpand::LookupError<_>> {
                        let args = <Result<_, _>>::from_iter(
                            args.into_iter()
                                .map(|arg| shellexpand::full(&arg).map(Into::into)),
                        )?;
                        Ok(Provider {
                            name,
                            command,
                            args,
                        })
                    },
                ),
        )?;
        let function = function
            .into_iter()
            .sorted_by(|f, g| f.name.cmp(&g.name))
            .dedup_by_with_count(|f, g| f.name == g.name)
            .inspect(|(count, function)| {
                if *count > 1 {
                    log::warn!("function {} defined {} times", function.name, count);
                }
                if !provider
                    .iter()
                    .any(|provider| provider.name == function.name)
                {
                    log::warn!("function {} has no provider", function.name);
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
            let mut spec = provider.specification()?;
            if let Some(function) = self.get_function(&spec.name) {
                merge(&mut spec, function);
            }
            Ok(spec)
        })
    }
}
