macro_rules! format_err {
    ($msg:literal $(,$args:expr)*) => {
        AssetError::FormatError(format!($msg, $($args),*))
    };
}

macro_rules! data_err {
    ($msg:literal $(,$args:expr)*) => {
        AssetError::DataError(format!($msg, $($args),*))
    };
}

#[derive(Debug)]
pub enum AssetError {
    /// An exception was raised within Racket.
    RacketException(String),

    /// The data format of an asset is incorrect.
    FormatError(String),

    /// The data for an asset is inconsistent.
    DataError(String),

    /// IO-Error
    IOError(std::io::Error),

    /// Something else went wrong.
    OtherError(String),
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RacketException(str) => write!(f, "Racket Exception: {}", str),
            Self::FormatError(str) => write!(f, "Format Error: {}", str),
            Self::DataError(str) => write!(f, "Data Error: {}", str),
            Self::IOError(err) => write!(f, "IO Error: {}", err),
            Self::OtherError(str) => write!(f, "Error: {}", str),
        }
    }
}

impl From<std::io::Error> for AssetError {
    fn from(err: std::io::Error) -> Self {
        AssetError::IOError(err)
    }
}
