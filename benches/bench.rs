use anyhow::anyhow;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use lithium::{catch, intercept, throw};

fn bench_anyhow(c: &mut Criterion) {
    fn rust() {
        fn imp(n: u32) {
            let n = black_box(n);
            if n == 0 {
                std::panic::resume_unwind(Box::new(anyhow!("Hello, world!")));
            } else {
                match std::panic::catch_unwind(|| imp(n - 1)) {
                    Ok(x) => x,
                    Err(mut bx) => {
                        let err = bx.downcast_mut::<anyhow::Error>().unwrap();
                        replace_with::replace_with_or_abort(err, |e| e.context("In imp"));
                        std::panic::resume_unwind(bx);
                    }
                }
            }
        }
        let _ = black_box(std::panic::catch_unwind(|| {
            imp(5);
        }));
    }

    fn lithium() {
        fn imp(n: u32) {
            let n = black_box(n);
            unsafe {
                if n == 0 {
                    throw(anyhow!("Hello, world!"));
                } else {
                    match intercept::<(), anyhow::Error>(|| imp(n - 1)) {
                        Ok(x) => x,
                        Err((e, in_flight)) => in_flight.rethrow(e.context("In imp")),
                    }
                }
            }
        }
        let _ = black_box(unsafe {
            catch::<(), anyhow::Error>(|| {
                imp(5);
            })
        });
    }

    let mut group = c.benchmark_group("anyhow");
    group.bench_function("Rust", |b| b.iter(|| rust()));
    group.bench_function("Lithium", |b| b.iter(|| lithium()));
    group.finish();
}

fn bench_simple(c: &mut Criterion) {
    fn rust() {
        fn imp(n: u32) {
            let n = black_box(n);
            if n == 0 {
                std::panic::resume_unwind(Box::new("Hello, world!"));
            } else {
                match std::panic::catch_unwind(|| imp(n - 1)) {
                    Ok(x) => x,
                    Err(mut bx) => {
                        let err = bx.downcast_mut::<&'static str>().unwrap();
                        *err = black_box(*err); // simulate adding information to the error in some fashion
                        std::panic::resume_unwind(bx);
                    }
                }
            }
        }
        let _ = black_box(std::panic::catch_unwind(|| {
            imp(5);
        }));
    }

    fn lithium() {
        fn imp(n: u32) {
            let n = black_box(n);
            unsafe {
                if n == 0 {
                    throw("Hello, world!");
                } else {
                    match intercept::<(), &'static str>(|| imp(n - 1)) {
                        Ok(x) => x,
                        Err((e, in_flight)) => in_flight.rethrow(black_box(e)), // simulate adding information
                    }
                }
            }
        }
        let _ = black_box(unsafe {
            catch::<(), &'static str>(|| {
                imp(5);
            })
        });
    }

    let mut group = c.benchmark_group("simple");
    group.bench_function("Rust", |b| b.iter(|| rust()));
    group.bench_function("Lithium", |b| b.iter(|| lithium()));
    group.finish();
}

criterion_group!(benches, bench_anyhow, bench_simple);
criterion_main!(benches);
