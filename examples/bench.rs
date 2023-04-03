use funcfmt::{FormatMap, Render, ToFormatPieces};
use std::fmt::Write;

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
        formatters.insert(i.to_string().into(), |e| Some(e.to_string()));
        if i % 3 == 0 {
            write!(&mut fmtstr, "{{{}}}", i).unwrap();
            write!(&mut expected, "bar").unwrap();
        }
    }

    for _ in 1..10000 {
        let fp = formatters.to_format_pieces(&fmtstr).unwrap();
        for _ in 1..1000 {
            let inp = String::from("bar");
            let fmt = fp.render(no_optim(&inp)).unwrap();
            assert_eq!(fmt, expected);
        }
    }
}
