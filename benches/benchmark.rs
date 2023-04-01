use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ftempl::*;
use std::fmt::Write;

fn criterion_benchmark(c: &mut Criterion) {
    let mut formatters: Vec<Formatter<String>> = Vec::new();
    let mut fmtstr = String::new();
    for i in 1..1000 {
        formatters.push(fm!(i, |e| Some(format!("_{e}_"))));
        write!(&mut fmtstr, "{{{}}}", i).unwrap();
    }
    c.bench_function("process_to_formatpieces", |b| {
        b.iter(|| process_to_formatpieces(black_box(&formatters), black_box(&fmtstr)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
