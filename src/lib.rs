use std::fmt::Write;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum FormatError {
    #[error("unknown field '{0}'")]
    UnknownField(String),

    #[error("no data for field '{0}'")]
    NoData(String),

    #[error("mismatched brackets in format")]
    MismatchedBrackets,

    #[error("integer overflow/underflow")]
    Overflow,

    #[error("fmt::Write error")]
    Write(#[from] std::fmt::Error),
}

pub type FormatterCallback<T> = fn(&T) -> Option<String>;

pub struct Formatter<T: ?Sized> {
    pub name: String,
    pub cb: FormatterCallback<T>,
}

pub enum FormatPiece<'a, T: ?Sized> {
    Char(char),
    Formatter(&'a Formatter<T>),
}

pub trait ToFormatPieces<T> {
    fn to_format_pieces<S: AsRef<str>>(
        &self,
        tmpl: S,
    ) -> Result<Vec<FormatPiece<'_, T>>, FormatError>;
}

impl<T> ToFormatPieces<T> for Vec<Formatter<T>> {
    fn to_format_pieces<S: AsRef<str>>(
        &self,
        tmpl: S,
    ) -> Result<Vec<FormatPiece<'_, T>>, FormatError> {
        // Need to be a bit careful to not index inside a character boundary
        let tmpl = tmpl.as_ref();
        let tmpl_vec = tmpl.chars().collect::<Vec<_>>();
        let mut chars = tmpl_vec.iter().enumerate().peekable();

        // Ballpark guesses large enough to usually avoid extra allocations
        let mut out: Vec<FormatPiece<T>> = Vec::with_capacity(tmpl.len());
        let mut start_word_idx = 0;

        while let Some((idx, cur)) = chars.next() {
            match (*cur, start_word_idx) {
                ('{', 0) => {
                    start_word_idx = idx.checked_add(1).ok_or(FormatError::Overflow)?;
                }
                ('{', s) if idx.checked_sub(s).ok_or(FormatError::Overflow)? == 0 => {
                    out.push(FormatPiece::Char(*cur));
                    start_word_idx = 0;
                }
                ('{', _) => return Err(FormatError::MismatchedBrackets),
                ('}', 0) if chars.next_if(|&(_, c)| c == &'}').is_some() => {
                    out.push(FormatPiece::Char(*cur));
                }
                ('}', 0) => return Err(FormatError::MismatchedBrackets),
                ('}', s) => {
                    let word = String::from_iter(&tmpl_vec[s..idx]);
                    match self.iter().find(|&f| f.name == word) {
                        Some(f) => out.push(FormatPiece::Formatter(f)),
                        None => return Err(FormatError::UnknownField(word)),
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

pub trait Render<T: ?Sized> {
    fn render(&self, data: &T) -> Result<String, FormatError>;
}

impl<T> Render<T> for Vec<FormatPiece<'_, T>> {
    fn render(&self, data: &T) -> Result<String, FormatError> {
        // Ballpark guess large enough to usually avoid extra allocations
        let mut out =
            String::with_capacity(self.len().checked_mul(4).ok_or(FormatError::Overflow)?);
        for piece in self {
            match *piece {
                FormatPiece::Char(c) => out.push(c),
                FormatPiece::Formatter(f) => write!(
                    &mut out,
                    "{}",
                    (f.cb)(data).ok_or_else(|| FormatError::NoData(f.name.to_string()))?
                )?,
            }
        }
        Ok(out)
    }
}

#[macro_export]
macro_rules! fm {
    ($name:tt, $cb:expr) => {
        $crate::Formatter {
            name: $name.to_string(),
            cb: $cb,
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazy_static::lazy_static;

    lazy_static! {
        static ref FORMATTERS: Vec<Formatter<String>> = vec![
            fm!("foo", |e| Some(format!("{e} foo {e}"))),
            fm!("bar", |e| Some(format!("{e} bar {e}"))),
            fm!("nodata", |_| None),
        ];
    }

    #[test]
    fn unicode_ok() {
        let inp = String::from("bar");
        let fp = FORMATTERS.to_format_pieces("一{foo}二{bar}").unwrap();
        let fmt = fp.render(&inp);
        assert_eq!(fmt, Ok("一bar foo bar二bar bar bar".to_owned()));
    }

    #[test]
    fn imbalance_open() {
        // Done in a somewhat weird way since FormatPiece is not PartialEq
        if let Err(err) = FORMATTERS.to_format_pieces("一{f{oo}二{bar}") {
            assert_eq!(err, FormatError::MismatchedBrackets);
            return;
        }
        panic!();
    }

    #[test]
    fn imbalance_close() {
        // Done in a somewhat weird way since FormatPiece is not PartialEq
        if let Err(err) = FORMATTERS.to_format_pieces("一{foo}}二{bar}") {
            assert_eq!(err, FormatError::MismatchedBrackets);
            return;
        }
        panic!();
    }

    #[test]
    fn imbalance_escaped() {
        let inp = String::from("bar");
        let fp = FORMATTERS.to_format_pieces("一{foo}二{{bar}}").unwrap();
        let fmt = fp.render(&inp);
        assert_eq!(fmt, Ok("一bar foo bar二{bar}".to_owned()));
    }

    #[test]
    fn unknown_field() {
        // Done in a somewhat weird way since FormatPiece is not PartialEq
        if let Err(err) = FORMATTERS.to_format_pieces("一{baz}二{bar}") {
            assert_eq!(err, FormatError::UnknownField("baz".to_string()));
            return;
        }
        panic!();
    }

    #[test]
    fn no_data() {
        let inp = String::from("bar");
        let fp = FORMATTERS.to_format_pieces("一{foo}二{nodata}").unwrap();
        assert_eq!(
            fp.render(&inp),
            Err(FormatError::NoData("nodata".to_string()))
        );
    }
}
