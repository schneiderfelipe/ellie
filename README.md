# ellie

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

ellie uses
- [async-openai](https://crates.io/crates/async-openai)
- [tiktoken-rs](https://crates.io/crates/tiktoken-rs)