use criterion::{black_box, criterion_group, criterion_main, Criterion};
use funcfmt::*;
use std::fmt::Write;

fn criterion_benchmark(c: &mut Criterion) {
    let mut formatters: FormatMap<String> = FormatMap::new();
    let mut fmtstr = String::new();
    for i in 1..1000 {
        formatters.insert(i.to_string().into(), |e| Some(format!("_{e}_")));
        write!(&mut fmtstr, "{{{}}}", i).unwrap();
    }

    c.bench_function("process_to_formatpieces", |b| {
        b.iter(|| formatters.to_format_pieces(black_box(&fmtstr)))
    });

    let fmtpieces = formatters.to_format_pieces(&fmtstr).unwrap();

    c.bench_function("render", |b| {
        b.iter(|| fmtpieces.render(black_box(&String::from("data"))))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
