use fnv::FnvHashMap;
use smallvec::SmallVec;
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
pub type FormatPieces<T> = SmallVec<[FormatPiece<T>; 256]>; // ~40b per FormatPiece<T>, ~10kb total

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
    Verbatim(SmartString<LazyCompact>),
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
    /// let fp = fmap.to_format_pieces("ab{foo}e").unwrap();
    /// let mut i = fp.iter();
    ///
    /// assert_eq!(i.next(), Some(&FormatPiece::Verbatim("ab".into())));
    /// assert!(matches!(i.next(), Some(FormatPiece::Formatter(_))));
    /// assert_eq!(i.next(), Some(&FormatPiece::Verbatim("e".into())));
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
        let chars = tmpl.char_indices();

        // Ballpark guesses large enough to usually avoid extra allocations
        let mut out = FormatPieces::with_capacity(tmpl.len());
        let mut start_key_idx = 0;
        let mut pending_escape = false;
        let mut last_pushed_idx = 0;

        macro_rules! push_verb {
            ($out:expr, $tmpl:expr, $range:expr) => {
                $out.push(FormatPiece::Verbatim($tmpl[$range].into()));
            };
        }

        for (idx, cur) in chars {
            match (cur, start_key_idx) {
                ('{', 0) => {
                    push_verb!(out, tmpl, last_pushed_idx..idx);
                    start_key_idx = idx.checked_add(1).ok_or(Error::Overflow)?;
                }
                ('{', s) if idx.checked_sub(s).ok_or(Error::Overflow)? == 0 => {
                    start_key_idx = 0;
                    last_pushed_idx = idx;
                }
                ('{', _) => return Err(Error::ImbalancedBrackets),
                ('}', 0) if !pending_escape => {
                    pending_escape = true;
                    push_verb!(out, tmpl, last_pushed_idx..idx);
                }
                ('}', 0) if pending_escape => {
                    pending_escape = false;
                    last_pushed_idx = idx;
                }
                ('}', s) => {
                    // SAFETY: We are already at idx and know it is valid, and s is definitely at
                    // a character boundary per .char_indices(). This is about a 2% speedup.
                    let key = unsafe { tmpl.get_unchecked(s..idx) };
                    let key = key.into();
                    match self.get(&key) {
                        Some(f) => {
                            out.push(FormatPiece::Formatter(Formatter { key, cb: f.clone() }));
                        }
                        None => return Err(Error::UnknownKey(key)),
                    };
                    start_key_idx = 0;
                    last_pushed_idx = idx.checked_add(1).ok_or(Error::Overflow)?;
                }

                _ => {
                    if pending_escape {
                        return Err(Error::ImbalancedBrackets);
                    }
                }
            }
        }

        if last_pushed_idx < tmpl.len() {
            push_verb!(out, tmpl, last_pushed_idx..);
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
                FormatPiece::Verbatim(s) => out.push_str(s),
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
