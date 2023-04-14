use funcfmt::{fm, Render, ToFormatPieces};

fn main() {
    let formatters = fm! {
        "foo" => |data| Some(format!("foo: {data}")),
        "bar" => |data| Some(format!("bar: {data}")),
        "baz" => |data| Some(format!("baz: {data}")),
    };

    let fp = formatters.to_format_pieces("{foo}, {bar}").unwrap();

    // foo: some data, bar: some data
    let data_one = String::from("some data");
    println!("{}", fp.render(&data_one).unwrap());

    // foo: other data, bar: other data
    // note that this doesn't require processing the format again
    let data_two = String::from("other data");
    println!("{}", fp.render(&data_two).unwrap());
}
