use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("UCUM expression must be ASCII")]
    NonAscii,

    #[error("UCUM expression must not contain whitespace")]
    ContainsWhitespace,

    #[error("invalid UCUM syntax at byte {pos}: {message}")]
    Syntax { pos: usize, message: &'static str },

    #[error("unknown unit symbol '{0}'")]
    UnknownUnit(String),

    #[error("unit '{0}' does not allow metric prefixes")]
    NotPrefixable(String),

    #[error("non-linear unit '{0}' is not convertible")]
    NonLinear(String),

    #[error("cannot apply exponent to affine unit '{0}'")]
    AffineExponent(String),

    #[error("incompatible units: '{from}' vs '{to}'")]
    Incompatible { from: String, to: String },

    #[error("unit database error: {0}")]
    Db(String),

    #[error("numeric overflow")]
    Overflow,
}
