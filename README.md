# Lithium

Lightweight exceptions.

Lithium provides a custom exception mechanism as an alternative to Rust panics. Compared to Rust panics, this mechanism is allocation-free, avoids indirections and RTTI, and hence faster, if less applicable.

Under the default configuration, Lithium is more than 2x faster Rust panics on common `Result`-like usecases. See the [benchmark](benches/bench.rs).

See [documentation](https://docs.rs/lithium) for usage and installation instructions.
