use fnv::FnvHashMap;
use smartstring::{LazyCompact, SmartString};
use std::fmt;
use std::sync::Arc;
use thiserror::Error;
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{anychar, char},
    combinator::map,
    multi::many0,
    sequence::{delimited, tuple},
    bytes::complete::take_while1,
    character::complete::alphanumeric1,
    IResult,
};

fn is_not_brace(c: char) -> bool {
    c != '{' && c != '}'
}

fn parse_format_piece<T>(input: &str) -> IResult<&str, FormatPiece<T>> {
    alt((
        map(take_while1(is_not_brace), |s: &str| {
            FormatPiece::Char(s.chars().next().unwrap())
        }),
        map(
            delimited(
                tuple((char('{'), char('{'))),
                anychar,
                tuple((char('}'), char('}'))),
            ),
            |c| FormatPiece::Char(c),
        ),
        map(
            delimited(char('{'), alphanumeric1, char('}')),
            |key: &str| FormatPiece::Formatter(Formatter {
                key: key.into(),
                cb: Arc::new(|_| None), // Placeholder, to be replaced later
            }),
        ),
    ))(input)
}

fn parse_format_pieces<T>(input: &str) -> IResult<&str, Vec<FormatPiece<T>>> {
    many0(parse_format_piece)(input)
}

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
pub struct Formatter<T> {
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
        let tmpl = tmpl.as_ref();
        let (_, pieces) = parse_format_pieces(tmpl).map_err(|_| Error::ImbalancedBrackets)?;

        let mut out = Vec::with_capacity(pieces.len());
        for piece in pieces {
            match piece {
                FormatPiece::Char(c) => out.push(FormatPiece::Char(c)),
                FormatPiece::Formatter(mut formatter) => {
                    if let Some(f) = self.get(&formatter.key) {
                        formatter.cb = f.clone();
                        out.push(FormatPiece::Formatter(formatter));
                    } else {
                        return Err(Error::UnknownKey(formatter.key.clone()));
                    }
                }
            }
        }
        Ok(out)
    }
}

/// A trait for rendering format pieces into a resulting `String`, given some input data to the
/// callbacks.
pub trait Render<T> {
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
mod lib_test;
