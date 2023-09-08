# ellie

[![Build Status]][actions]
[![Latest Version]][crates.io]
[![Documentation]][docs.rs]

[Build Status]: https://github.com/schneiderfelipe/ellie/actions/workflows/rust.yml/badge.svg
[actions]: https://github.com/schneiderfelipe/ellie/actions/workflows/rust.yml
[Latest Version]: https://img.shields.io/crates/v/ellie.svg
[crates.io]: https://crates.io/crates/ellie
[Documentation]: https://img.shields.io/docsrs/ellie
[docs.rs]: https://docs.rs/ellie

Urban broccoli.

## Draft

This is how ellie will work when complete:

```console
$ echo Tell me a joke | ellie
Q: Why did the tomato turn red?
A: Because it saw the salad dressing!
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

### Inner workings

ellie makes decisions in a simplified matter,
so that you don't have to.
ellie is simple,
and should simply work.

It makes decisions based on its input,
prepares a request based on it,
processes the request,
gets a response,
and gives you an answer in the end.

ellie uses
- [async-openai](https://crates.io/crates/async-openai)
- [tiktoken-rs](https://crates.io/crates/tiktoken-rs)

## Functions

Function calling is supported by delegating to external providers
(i.e.,
a script or binary).
All you have to do is configure a function provider in `~/.config/ellie/functions.toml` (or the equivalent path in your platform):

```toml
[[provider]]
name = "get_current_weather"
command = "python"
args = ["get_current_weather.py"]
```

### Providers

A function provider reads from the standard input and writes results to the standard output.
Additionally,
when given an extra `spec` argument,
it writes a specification to the standard output.
So the behavior for the example above would be as follows:

```console
$ python get_current_weather.py spec
{"name": "get_current_weather", "description": "Get the current weather in a given location", "parameters": {"type": "object", "required": ["location"], "properties": {"location": {"type": "string", "description": "The city and state, e.g. San Francisco, CA"}, "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}}}}

$ echo '{"location":"Boston, MA"}' | python get_current_weather.py
{"location": "Boston, MA", "temperature": "72", "unit": null, "forecast": ["sunny", "windy"]}
```

A template implementation in Python would be as follows:

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

Of course,
you could write function providers in any programming language.
For more on function specifications,
take a look at the [OpenAI official guide](https://platform.openai.com/docs/guides/gpt/function-calling).

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
