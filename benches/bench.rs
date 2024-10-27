use anyhow::{anyhow, Error};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lithium::{catch, intercept, throw};

fn rust() {
    fn imp(n: u32) {
        let n = black_box(n);
        if n == 0 {
            std::panic::resume_unwind(Box::new(anyhow!("Hello, world!")));
        } else {
            let mut bx = std::panic::catch_unwind(|| imp(n - 1)).unwrap_err();
            let err = bx.downcast_mut::<Error>().unwrap();
            replace_with::replace_with_or_abort(err, |e| e.context("In imp"));
            std::panic::resume_unwind(bx);
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
                let (e, in_flight) = intercept::<(), Error>(|| imp(n - 1)).unwrap_err();
                in_flight.rethrow(e.context("In imp"));
            }
        }
    }
    let _ = black_box(unsafe {
        catch::<(), Error>(|| {
            imp(5);
        })
    });
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("Exceptions");
    group.bench_function("Rust", |b| b.iter(|| rust()));
    group.bench_function("Lithium", |b| b.iter(|| lithium()));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
