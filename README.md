# Lithium

[![License](https://img.shields.io/crates/l/lithium)](LICENSE) [![Version](https://img.shields.io/crates/v/lithium)](https://crates.io/crates/lithium) [![docs.rs](https://img.shields.io/docsrs/lithium)](https://docs.rs/lithium)

Lightweight exceptions.

Lithium provides a custom exception mechanism as an alternative to Rust panics. Compared to Rust panics, this mechanism is allocation-free, avoids indirections and RTTI, and hence faster, if less applicable.

Under the default configuration, Lithium is more than 2x faster Rust panics on common `Result`-like usecases. See the [benchmark](benches/bench.rs).

See [documentation](https://docs.rs/lithium) for usage and installation instructions.
