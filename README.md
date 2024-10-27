# Lithium

Lightweight exceptions.

Lithium provides a custom exception mechanism as an alternative to Rust panics. Compared to Rust panics, this mechanism is allocation-free, avoids indirections and RTTI, and hence faster, if less applicable.

When used together with the [`revolve`](https://lib.rs/revolve) crate, unwinding exceptions takes only 3 ns/frame, barely slower than returns. In this configuration, using exceptions instead of `Result` both speeds up the success path and only marginally slows down the error path, making this approach ideal when the probability of an error is low.

See [documentation](https://docs.rs/lithium) for usage and installation instructions.
