# funcfmt | [![Tests](https://img.shields.io/github/actions/workflow/status/cdown/funcfmt/ci.yml?branch=master)](https://github.com/cdown/funcfmt/actions?query=branch%3Amaster)

funcfmt is a simple, lightweight templating library that allows templating
using custom runtime context and function pointers. It was originally created
for [exifrename](https://github.com/cdown/exifrename), to allow efficiently
processing a format and set of callbacks across thousands of EXIF objects.

## Usage

To add funcfmt to your dependencies:

```
cargo add funcfmt
```

The basic flow of funcfmt looks like this:

1. Given a `FormatMap<T>` called `formatters`, call
   `formatters.to_format_pieces()`, which preprocesses everything into a
   `FormatPieces<T>`, where `&T` is what your callback function will take as
   its only argument. This allows avoiding having to reparse the formatters and
   go through the template each time things are processed.
2. Call .render(data) on the `FormatPieces<T>`.

A very small example with `String`s passed in, although you can pass an object
of any type:

```rust
use funcfmt::{fm, FormatMap, Render, ToFormatPieces};

fn main() {
    let formatters = FormatMap::from([
        fm!("foo", |data| Some(format!("foo: {data}"))),
        fm!("bar", |data| Some(format!("bar: {data}"))),
        fm!("baz", |data| Some(format!("baz: {data}"))),
    ]);

    let fp = formatters.to_format_pieces("{foo}, {bar}").unwrap();

    // foo: some data, bar: some data
    let data_one = String::from("some data");
    println!("{}", fp.render(&data_one).unwrap());

    // foo: other data, bar: other data
    // note that this doesn't require processing the format again
    let data_two = String::from("other data");
    println!("{}", fp.render(&data_two).unwrap());
}
```
