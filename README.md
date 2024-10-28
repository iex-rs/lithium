# Lithium

Lightweight exceptions.

Lithium provides a custom exception mechanism as an alternative to Rust panics. Compared to Rust panics, this mechanism is allocation-free, avoids indirections and RTTI, and hence faster, if less applicable.

Under the default configuration, Lithium is more than 2x faster Rust panics on common `Result`-like usecases. When used together with the [`revolve`](https://lib.rs/revolve) crate, it's 2-4x faster than Rust panics, bringing timings down to about 10 ns per frame with `map_err` in realistic code -- faster than an allocation. See the [benchmark](benches/bench.rs).

See [documentation](https://docs.rs/lithium) for usage and installation instructions.
