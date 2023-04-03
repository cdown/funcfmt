use funcfmt::{FormatMap, Render, ToFormatPieces};
use std::fmt::Write;

fn main() {
    let mut formatters: FormatMap<String> = FormatMap::new();
    let mut fmtstr = String::new();
    let mut expected = String::new();

    for i in 1..10000000 {
        formatters.insert(i.to_string(), |e| Some(format!("_{e}_")));
        write!(&mut fmtstr, "{{{}}}", i).unwrap();
        write!(&mut expected, "_bar_").unwrap();
    }

    let fp = formatters.to_format_pieces(&fmtstr).unwrap();
    let inp = String::from("bar");
    let fmt = fp.render(&inp).unwrap();
    println!("fmt == expected: {}", fmt == expected);
}
