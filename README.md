# ellie

[![Build Status]][actions]
[![Latest Version]][crates.io]
[![Documentation]][docs.rs]

```console
$ echo "It's dangerous to go alone" | ellie
Take this!
```

ellie is a suckless,
opinionated,
text-in-text-out command-line application.

ellie chooses models for you.
ellie chooses system messages for you.
ellie chooses temperature for you.
ellie chooses top p for you.
ellie chooses maximum number of tokens for you.
ellie chooses stop sequences for you.

ellie makes decisions in a simplified matter,
so that you don't have to.
ellie is simple,
and should simply work.

It makes decisions based on its input,
prepares a request based on it,
processes the request,
gets a response,
and gives you an answer in the end.

## Functions

Function calling is supported by delegating to external providers.
All you have to do is configure a function provider in `~/.config/ellie/functions.toml` (or the equivalent path in your platform).

### Provider configuration

To configure a function provider,
add the following information to the `functions.toml` file:

```toml
[[provider]]
name = "get_current_weather"
command = "python"
args = ["get_current_weather.py"]
```

This example configures a provider named "get_current_weather" that uses a Python script called "get_current_weather.py".

### Provider behavior

A function provider reads from the standard input and
writes results to the standard output.
When given an extra `spec` argument,
it writes a specification to the standard output.

For example,
running the provider with the `spec` argument would output the following specification:

```console
$ python get_current_weather.py spec
{
  "name": "get_current_weather",
  "description": "Get the current weather in a given location",
  "parameters": {
    "type": "object",
    "required": ["location"],
    "properties": {
      "location": {
        "type": "string",
        "description": "The city and state, e.g. San Francisco, CA"
      },
      "unit": {
        "type": "string",
        "enum": ["celsius", "fahrenheit"]
      }
    }
  }
}
```

To call the function,
you can provide the required input as a JSON object through the standard input.
The provider will then output the result as a JSON object.

For example:

```console
$ echo '{"location":"Boston, MA"}' | python get_current_weather.py
{
  "location": "Boston, MA",
  "temperature": "72",
  "unit": null,
  "forecast": ["sunny", "windy"]
}
```

### Template implementation

Here is a template implementation in Python:

```python
from sys import argv
import json

match argv[1:]:
    case []:
        print(json.dumps({
            # ...
        }))
    case ["spec"]:
        print(json.dumps({
            # ...
        }))
```

You can write function providers in any programming language.
For more information on function specifications,
refer to the [OpenAI official guide](https://platform.openai.com/docs/guides/gpt/function-calling).

## Detailed output

**TL;DR**: use logging.

If you just want to see functions being called,
use `RUST_LOG=info`:

```console
$ echo 'What is the weather like in Boston?' | RUST_LOG=info ellie
 INFO  ellie > get_current_weather {"location":"Boston, MA"}
The weather in Boston is currently sunny and windy with a temperature of 72 degrees.
```

For debugging information (e.g.,
the exact request payloads),
use `RUST_LOG=debug`:

```console
$ echo 'What is the weather like in Boston?' | RUST_LOG=debug ellie
 DEBUG ellie > {"model":"gpt-3.5-turbo","messages":[{"role":"user","content":"What is the weather like in Boston?"}],"functions":[{"name":"get_current_weather","description":"Get the current weather in a given location","parameters":{"properties":{"location":{"description":"The city and state, e.g. San Francisco, CA","type":"string"},"unit":{"enum":["celsius","fahrenheit"],"type":"string"}},"required":["location"],"type":"object"}}],"temperature":0.0,"max_tokens":null}
 INFO  ellie > get_current_weather {"location":"Boston, MA"}
 DEBUG ellie > {"model":"gpt-3.5-turbo","messages":[{"role":"user","content":"What is the weather like in Boston?"},{"role":"assistant","content":"","function_call":{"name":"get_current_weather","arguments":"{\"location\":\"Boston, MA\"}"}},{"role":"function","content":"{\"forecast\":[\"sunny\",\"windy\"],\"location\":\"Boston, MA\",\"temperature\":\"72\",\"unit\":null}","name":"get_current_weather"}],"functions":[{"name":"get_current_weather","description":"Get the current weather in a given location","parameters":{"properties":{"location":{"description":"The city and state, e.g. San Francisco, CA","type":"string"},"unit":{"enum":["celsius","fahrenheit"],"type":"string"}},"required":["location"],"type":"object"}}],"temperature":0.0,"max_tokens":null}
The weather in Boston is currently sunny and windy with a temperature of 72 degrees.
```

[actions]: https://github.com/schneiderfelipe/ellie/actions/workflows/rust.yml
[build status]: https://github.com/schneiderfelipe/ellie/actions/workflows/rust.yml/badge.svg
[crates.io]: https://crates.io/crates/ellie
[docs.rs]: https://docs.rs/ellie
[documentation]: https://img.shields.io/docsrs/ellie
[latest version]: https://img.shields.io/crates/v/ellie.svg
