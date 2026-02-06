/// Result type for a `LengthError`.
pub type LengthResult<T> = Result<T, Error>;

/// Enumerates a set of length related errors.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum LengthErrorKind {
    /// Length exceeded an expected size.
    LengthExceeded,
    /// Length is not equal to an expected size.
    LengthExpected,
    /// Length is not a multiple of an expected size.
    LengthMultipleExpected,
}

/// Generic length error for various types.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Error {
    kind: LengthErrorKind,
    length: usize,
    index: Option<usize>,
}

impl Error {
    /// Create a `LengthError`.
    #[must_use]
    pub const fn new(kind: LengthErrorKind, length: usize) -> Self {
        Self {
            kind,
            length,
            index: None,
        }
    }

    /// Create a `LengthError` for a given element index.
    #[must_use]
    pub const fn with_index(kind: LengthErrorKind, length: usize, index: usize) -> Self {
        Self {
            kind,
            length,
            index: Some(index),
        }
    }

    /// Error is with the given length/length multiple.
    #[must_use]
    pub const fn length(&self) -> usize {
        self.length
    }

    /// Error is for the element at the given index.
    #[must_use]
    pub const fn index(&self) -> Option<usize> {
        self.index
    }
}
