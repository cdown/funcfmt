use fnv::FnvHashMap;
use smartstring::{LazyCompact, SmartString};
use std::borrow::Borrow;
use std::fmt;
use std::sync::Arc;
use thiserror::Error;
use std::marker::PhantomData;

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

#[derive(Clone)]
pub struct FormatterCallback<T: ?Sized, F: Fn(&T) -> Option<String> + Send + Sync + 'static> {
    f: Arc<F>,
    _marker: PhantomData<T>,
}

impl<T: ?Sized, F: Fn(&T) -> Option<String> + Send + Sync + 'static> FormatterCallback<T, F> {
    pub fn new(f: F) -> Self {
        Self {
            f: Arc::new(f),
            _marker: PhantomData,
        }
    }

    pub fn call<B: Borrow<T>>(&self, arg: B) -> Option<String> {
        (self.f)(arg.borrow())
    }
}

/// A mapping of keys to callback functions.
pub type FormatMap<T, F> = FnvHashMap<SmartString<LazyCompact>, FormatterCallback<T, F>>;

/// A container of either plain `Char`s or function callbacks to be called later in `render`.
pub type FormatPieces<T, F> = Vec<FormatPiece<T, F>>;

/// A container around the callback that also contains the name of the key.
pub struct Formatter<T: ?Sized, F: Fn(&T) -> Option<String> + Send + Sync + 'static> {
    pub key: SmartString<LazyCompact>,
    pub cb: Arc<F>,
}

impl<T, F: Fn(&T) -> Option<String> + Send + Sync> PartialEq for Formatter<T, F> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl<T, F: Fn(&T) -> Option<String> + Send + Sync> Eq for Formatter<T, F> {}

impl<T, F: Fn(&T) -> Option<String> + Send + Sync> fmt::Debug for Formatter<T, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Formatter(key: {})", self.key)
    }
}

/// Either a plain `Char`, or a function call back to be called later in `render`.
#[derive(PartialEq, Eq, Debug)]
pub enum FormatPiece<T, F: Fn(&T) -> Option<String> + Send + Sync + 'static> {
    Char(char),
    Formatter(Formatter<T, F>),
}

// A trait for processing a sequence of formatters and given template into a `FormatPieces<T>`.
pub trait ToFormatPieces<T, F: Fn(&T) -> Option<String> + Send + Sync> {
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
    fn to_format_pieces<S: AsRef<str>>(&self, tmpl: S) -> Result<FormatPieces<T, F>, Error>;
}

impl<T, F: Fn(&T) -> Option<String> + Send + Sync> ToFormatPieces<T, F> for FormatMap<T, F> {
    fn to_format_pieces<S: AsRef<str>>(&self, tmpl: S) -> Result<FormatPieces<T, F>, Error> {
        // Need to be a bit careful to not index inside a character boundary
        let tmpl = tmpl.as_ref();
        let chars = tmpl.char_indices();

        // Ballpark guesses large enough to usually avoid extra allocations
        let mut out = FormatPieces::with_capacity(tmpl.len());
        let mut start_word_idx = 0;
        let mut pending_escape = false;

        for (idx, cur) in chars {
            match (cur, start_word_idx) {
                ('{', 0) => {
                    start_word_idx = idx.checked_add(1).ok_or(Error::Overflow)?;
                }
                ('{', s) if idx.checked_sub(s).ok_or(Error::Overflow)? == 0 => {
                    out.push(FormatPiece::Char(cur));
                    start_word_idx = 0;
                }
                ('{', _) => return Err(Error::ImbalancedBrackets),
                ('}', 0) if !pending_escape => pending_escape = true,
                ('}', 0) if pending_escape => {
                    out.push(FormatPiece::Char(cur));
                    pending_escape = false;
                }
                ('}', s) => {
                    // SAFETY: We are already at idx and know it is valid, and s is definitely at
                    // a character boundary per .char_indices(). This is about a 2% speedup.
                    let word = unsafe { tmpl.get_unchecked(s..idx) };
                    let word = word.into();
                    match self.get(&word) {
                        Some(f) => {
                            out.push(FormatPiece::Formatter(Formatter {
                                key: word,
                                cb: f.f.clone(),
                            }));
                        }
                        None => return Err(Error::UnknownKey(word)),
                    };
                    start_word_idx = 0;
                }

                (_, _) if pending_escape => return Err(Error::ImbalancedBrackets),
                (_, s) if s > 0 => {}
                (c, _) => out.push(FormatPiece::Char(c)),
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
    fn render<D: Borrow<T>>(&self, data: D) -> Result<String, Error>;
}

impl<T, F: Fn(&T) -> Option<String> + Send + Sync> Render<T> for FormatPieces<T, F> {
    fn render<D: Borrow<T>>(&self, data: D) -> Result<String, Error> {
        // Ballpark guess large enough to usually avoid extra allocations
        let mut out = String::with_capacity(self.len().checked_mul(16).ok_or(Error::Overflow)?);
        for piece in self {
            match piece {
                FormatPiece::Char(c) => out.push(*c),
                FormatPiece::Formatter(f) => {
                    out.push_str(
                        &(f.cb)(data.borrow()).ok_or_else(|| Error::NoData(f.key.clone()))?,
                    );
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
            let cb: $crate::FormatterCallback::new($value);
            map.insert($key.into(), cb);
        )*
        map
    }};
}

#[cfg(test)]
mod lib_test;
