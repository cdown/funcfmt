use super::*;
use once_cell::sync::Lazy;
use proptest::prelude::*;

static FORMATTERS: Lazy<FormatMap<String>> = Lazy::new(|| {
    fm! {
        "foo" => |e| Some(format!("{e} foo {e}")),
        "bar" => |e| Some(format!("{e} bar {e}")),
        "nodata" => |_| None,
    }
});

proptest! {
    // \PC == invisible control characters and unused code points, the opposite of \pC
    // tmpl: Any \PC which is not { or }, surrounding {foo} and {bar} one or more times.
    #[test]
    fn unicode_ok(
        tmpl in r#"[^\p{C}{}]*\{foo\}[^\p{C}{}]*\{bar\}[^\p{C}{}]*"#,
        inp in r#"\PC*"#,
    ) {
        let fp = FORMATTERS.to_format_pieces(tmpl).unwrap();
        let fmt = fp.render(&inp).unwrap();
        prop_assert!(fmt.contains(" foo "));
        prop_assert!(fmt.contains(" bar "));
        prop_assert!(fmt.contains(&inp));
    }
}

#[test]
fn imbalance_open() {
    assert_eq!(
        FORMATTERS.to_format_pieces("一{f{oo}二{bar}"),
        Err(Error::ImbalancedBrackets)
    );
}

#[test]
fn imbalance_close() {
    assert_eq!(
        FORMATTERS.to_format_pieces("一{foo}}二{bar}"),
        Err(Error::ImbalancedBrackets)
    );
}

#[test]
fn imbalance_escaped() {
    let inp = String::from("bar");
    let fp = FORMATTERS.to_format_pieces("一{foo}二{{bar}}").unwrap();
    let fmt = fp.render(&inp);
    assert_eq!(fmt, Ok("一bar foo bar二{bar}".to_owned()));
}

#[test]
fn unknown_key() {
    assert_eq!(
        FORMATTERS.to_format_pieces("一{baz}二{bar}"),
        Err(Error::UnknownKey("baz".into()))
    );
}

#[test]
fn no_data() {
    let inp = String::from("bar");
    let fp = FORMATTERS.to_format_pieces("一{foo}二{nodata}").unwrap();
    assert_eq!(fp.render(&inp), Err(Error::NoData("nodata".into())));
}

#[test]
fn error_converts() {
    let error = Error::ImbalancedBrackets;
    let error: Box<dyn std::error::Error> = Box::new(error);
    assert!(error.source().is_none());
    assert_eq!(
        error.downcast_ref::<Error>(),
        Some(&Error::ImbalancedBrackets)
    );
}

#[test]
fn error_from_fmt_error() {
    assert_eq!(Error::from(std::fmt::Error), Error::Write(std::fmt::Error));
}

#[test]
fn error_display() {
    assert_eq!(
        Error::Write(std::fmt::Error).to_string(),
        "std::fmt::Write error"
    );
}

#[test]
fn formatter_eq_based_on_key_only() {
    let c1: FormatterCallback<String> = Arc::new(|e| Some(e.to_string()));
    let c2: FormatterCallback<String> = Arc::new(|e| Some(e.to_string()));

    let f1 = Formatter {
        key: "foo".into(),
        cb: c1.clone(),
    };
    let f2 = Formatter {
        key: "foo".into(),
        cb: c2,
    };
    let b1 = Formatter {
        key: "bar".into(),
        cb: c1,
    };

    assert_eq!(f1, f2);
    assert_ne!(f1, b1);
}

#[test]
fn formatter_debug() {
    let c1: FormatterCallback<String> = Arc::new(|e| Some(e.to_string()));
    let f1 = Formatter {
        key: "foo".into(),
        cb: c1,
    };
    assert_eq!(format!("{:?}", f1), "Formatter(key: foo)");
}
