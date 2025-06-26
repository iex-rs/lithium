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
        unsafe fn imp(n: u32) {
            let n = black_box(n);
            if n == 0 {
                unsafe {
                    throw(anyhow!("Hello, world!"));
                }
            } else {
                match intercept::<(), anyhow::Error>(|| unsafe { imp(n - 1) }) {
                    Ok(x) => x,
                    Err((e, handle)) => unsafe { handle.rethrow(e.context("In imp")) },
                }
            }
        }
        let _ = black_box(catch::<(), anyhow::Error>(|| unsafe {
            imp(5);
        }));
    }

    let mut group = c.benchmark_group("anyhow");
    group.bench_function("Rust", |b| b.iter(rust));
    group.bench_function("Lithium", |b| b.iter(lithium));
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
        unsafe fn imp(n: u32) {
            let n = black_box(n);
            if n == 0 {
                unsafe {
                    throw("Hello, world!");
                }
            } else {
                match intercept::<(), &'static str>(|| unsafe { imp(n - 1) }) {
                    Ok(x) => x,
                    Err((e, handle)) => unsafe { handle.rethrow(black_box(e)) }, // simulate adding information
                }
            }
        }
        let _ = black_box(catch::<(), &'static str>(|| unsafe {
            imp(5);
        }));
    }

    let mut group = c.benchmark_group("simple");
    group.bench_function("Rust", |b| b.iter(rust));
    group.bench_function("Lithium", |b| b.iter(lithium));
    group.finish();
}

criterion_group!(benches, bench_anyhow, bench_simple);
criterion_main!(benches);
