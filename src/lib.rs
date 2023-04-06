use fnv::FnvHashMap;
use smartstring::{LazyCompact, SmartString};
use std::fmt;
use std::sync::Arc;
use thiserror::Error;

/// An error produced during formatting.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum Error {
    /// A key was requested, but it has no entry in the provided `FormatMap<T>`. Stores the key
    /// name which was unknown.
    #[error("unknown key '{0}'")]
    UnknownKey(SmartString<LazyCompact>),

    /// No data available for a callback. Stores the key name which had no data available, i.e.,
    /// the callback returned `None`.
    #[error("no data for key '{0}'")]
    NoData(SmartString<LazyCompact>),

    /// The template provided had imbalanced brackets. If you want to escape { or }, use {{ or }}
    /// respectively.
    #[error("imbalanced brackets in template")]
    ImbalancedBrackets,

    /// An integer overflowed or underflowed internally.
    #[error("integer overflow/underflow")]
    Overflow,

    /// An error occurred during writing the result of the closure to the eventual output `String`.
    /// Stores the encapsulated error.
    #[error("std::fmt::Write error")]
    Write(#[from] std::fmt::Error),
}

/// A callback to be provided with data during rendering.
pub type FormatterCallback<T> = Arc<dyn Fn(&T) -> Option<String> + Send + Sync>;

/// A mapping of keys to callback functions.
pub type FormatMap<T> = FnvHashMap<SmartString<LazyCompact>, FormatterCallback<T>>;

/// A container of either plain `Char`s or function callbacks to be called later in `render`.
pub type FormatPieces<T> = Vec<FormatPiece<T>>;

/// A container around the callback that also contains the name of the key.
pub struct Formatter<T: ?Sized> {
    pub key: SmartString<LazyCompact>,
    pub cb: FormatterCallback<T>,
}

impl<T> PartialEq for Formatter<T> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl<T> Eq for Formatter<T> {}

impl<T> fmt::Debug for Formatter<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Formatter(key: {})", self.key)
    }
}

/// Either a plain `Char`, or a function call back to be called later in `render`.
#[derive(PartialEq, Eq, Debug)]
pub enum FormatPiece<T> {
    Char(char),
    Formatter(Formatter<T>),
}

/// A trait for processing a sequence of formatters and given template into a `FormatPieces<T>`.
pub trait ToFormatPieces<T> {
    /// Processes the given value into a `FormatPieces<T>`.
    ///
    /// # Template format
    ///
    /// The template `tmpl` takes keys in the format `{foo}`, which will be replaced with the output
    /// from the callback registered to key "foo". Callbacks return an `Option<String>`.
    ///
    /// If you want to return literal "{foo}", pass `{{foo}}`.
    ///
    /// There are no restrictions on key names, other than that they cannot contain "{" or "}".
    /// This is not enforced at construction time, but trying to use them will fail with
    /// `Error::ImbalancedBrackets`.
    ///
    /// # Example
    ///
    /// ```
    /// use std::matches;
    /// use funcfmt::{FormatMap, ToFormatPieces, fm, FormatPiece, FormatterCallback};
    ///
    /// let fmap: FormatMap<String> = fm!(("foo", |data| Some(format!("b{data}d"))));
    /// let fp = fmap.to_format_pieces("a{foo}e").unwrap();
    /// let mut i = fp.iter();
    ///
    /// assert_eq!(i.next(), Some(&FormatPiece::Char('a')));
    /// assert!(matches!(i.next(), Some(FormatPiece::Formatter(_))));
    /// assert_eq!(i.next(), Some(&FormatPiece::Char('e')));
    /// ```
    ///
    /// # Errors
    ///
    /// - `Error::ImbalancedBrackets` if `tmpl` contains imbalanced brackets (use `{{` and `}}` to
    ///    escape)
    /// - `Error::Overflow` if internal string capacity calculation overflows
    /// - `Error::UnknownKey` if a requested key has no associated callback
    fn to_format_pieces<S: AsRef<str>>(&self, tmpl: S) -> Result<FormatPieces<T>, Error>;
}

impl<T> ToFormatPieces<T> for FormatMap<T> {
    fn to_format_pieces<S: AsRef<str>>(&self, tmpl: S) -> Result<FormatPieces<T>, Error> {
        // Need to be a bit careful to not index inside a character boundary
        let tmpl = tmpl.as_ref();
        let mut chars = tmpl.char_indices().peekable();

        // Ballpark guesses large enough to usually avoid extra allocations
        let mut out = FormatPieces::with_capacity(tmpl.len());
        let mut start_word_idx = 0;

        while let Some((idx, cur)) = chars.next() {
            match (cur, start_word_idx) {
                ('{', 0) => {
                    start_word_idx = idx.checked_add(1).ok_or(Error::Overflow)?;
                }
                ('{', s) if idx.checked_sub(s).ok_or(Error::Overflow)? == 0 => {
                    out.push(FormatPiece::Char(cur));
                    start_word_idx = 0;
                }
                ('{', _) => return Err(Error::ImbalancedBrackets),
                ('}', 0) if chars.next_if(|&(_, c)| c == '}').is_some() => {
                    out.push(FormatPiece::Char(cur));
                }
                ('}', 0) => return Err(Error::ImbalancedBrackets),
                ('}', s) => {
                    // SAFETY: We are already at idx and know it is valid, and s is definitely at
                    // a character boundary per .char_indices().
                    let word = unsafe { tmpl.get_unchecked(s..idx) };
                    let word = word.into();
                    match self.get(&word) {
                        Some(f) => {
                            out.push(FormatPiece::Formatter(Formatter {
                                key: word,
                                cb: f.clone(),
                            }));
                        }
                        None => return Err(Error::UnknownKey(word)),
                    };
                    start_word_idx = 0;
                }

                (_, s) if s > 0 => {}
                (c, _) => out.push(FormatPiece::Char(c)),
            }
        }

        Ok(out)
    }
}

/// A trait for rendering format pieces into a resulting `String`, given some input data to the
/// callbacks.
pub trait Render<T: ?Sized> {
    /// Given some data, render the given format pieces into a `String`.
    ///
    /// # Example
    ///
    /// ```
    /// use funcfmt::{FormatMap, ToFormatPieces, Render, fm};
    ///
    /// let fmap = fm!(("foo", |data| Some(format!("b{data}d"))));
    /// let fp = fmap.to_format_pieces("a{foo}e").unwrap();
    /// let data = String::from("c");
    /// assert_eq!(fp.render(&data), Ok("abcde".to_string()));
    /// ```
    ///
    /// # Errors
    ///
    /// - `Error::NoData` if the callback returns `None`
    /// - `Error::Overflow` if internal string capacity calculation overflows
    /// - `Error::Write` if writing to the output `String` fails
    fn render(&self, data: &T) -> Result<String, Error>;
}

impl<T> Render<T> for FormatPieces<T> {
    fn render(&self, data: &T) -> Result<String, Error> {
        // Ballpark guess large enough to usually avoid extra allocations
        let mut out = String::with_capacity(self.len().checked_mul(16).ok_or(Error::Overflow)?);
        for piece in self {
            match piece {
                FormatPiece::Char(c) => out.push(*c),
                FormatPiece::Formatter(f) => {
                    out.push_str(&(f.cb)(data).ok_or_else(|| Error::NoData(f.key.clone()))?);
                }
            }
        }
        Ok(out)
    }
}

/// Convenience macro to construct a single mapping for a `FormatMap`, since the types are somewhat
/// complex.
///
/// # Example
///
/// ```
/// use funcfmt::{fm, FormatMap};
///
/// let fmap: FormatMap<String> = fm!(("foo", |data| Some(format!("b{data}d"))));
/// ```
#[macro_export]
macro_rules! fm {
    ( $( ($key:expr, $value:expr) ),* $(,)?) => {{
        let mut map = $crate::FormatMap::default();
        $(
            let cb: $crate::FormatterCallback<_> = std::sync::Arc::new($value);
            map.insert($key.into(), cb);
        )*
        map
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;

    static FORMATTERS: Lazy<FormatMap<String>> = Lazy::new(|| {
        fm!(
            ("foo", |e| Some(format!("{e} foo {e}"))),
            ("bar", |e| Some(format!("{e} bar {e}"))),
            ("nodata", |_| None)
        )
    });

    #[test]
    fn unicode_ok() {
        let inp = String::from("bar");
        let fp = FORMATTERS.to_format_pieces("一{foo}二{bar}").unwrap();
        let fmt = fp.render(&inp);
        assert_eq!(fmt, Ok("一bar foo bar二bar bar bar".to_owned()));
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
}
