# func_template

func_template allows templating using custom runtime context and function
pointers. It was originally created for
[exifrename](https://github.com/cdown/exifrename).

## Usage

The basic flow of func_template looks like this:

1. Given a `FormatMap<T>` called `formatters`, call
   `formatters.to_format_pieces()`, which preprocesses everything into a
   `Vec<FormatPiece<T>>`, where `&T` is what your callback function will take
   as its only argument. This allows avoiding having to reparse the formatters
   and go through the template each time things are processed.
2. Call .render(data) on the `Vec<FormatPiece<T>>`.
