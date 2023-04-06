use funcfmt::{FormatMap, Render, ToFormatPieces};
use std::fmt::Write;
use std::option_env;
use std::sync::Arc;

fn no_optim<T>(data: T) -> T {
    unsafe {
        let ret = std::ptr::read_volatile(&data);
        std::mem::forget(data);
        ret
    }
}

fn main() {
    let mut formatters: FormatMap<String> = FormatMap::new();
    let mut fmtstr = String::new();
    let mut expected = String::new();

    // exifrename-like performance case, ran 10000 times
    //
    // - About 20 tags
    // - A normal query uses maybe 1/3 of these
    // - And you run over about 1000 files or so

    for i in 1..20 {
        formatters.insert(
            i.to_string().into(),
            Arc::new(no_optim(|e: &String| Some(e.to_string()))),
        );
        if i % 3 == 0 {
            write!(&mut fmtstr, "{{{}}}", i).unwrap();
            write!(&mut expected, "bar").unwrap();
        }
    }

    let fp_only = option_env!("FP_ONLY").is_some();

    let rounds = if fp_only { 100000 } else { 1000 };

    for _ in 1..rounds {
        let fp = formatters.to_format_pieces(&fmtstr).unwrap();
        if fp_only {
            continue;
        }
        for _ in 1..1000 {
            let inp = String::from("bar");
            let fmt = fp.render(no_optim(&inp)).unwrap();
            assert_eq!(fmt, expected);
        }
    }
}
