use derive_more::From;
use std::path::PathBuf;

pub type Result<T = ()> = std::result::Result<T, Error>;

#[derive(Debug, From)]
pub enum Error {
    #[from]
    Git(git2::Error),

    #[from]
    Io(std::io::Error),

    #[from]
    Image(image::ImageError),

    #[from]
    Camera(nokhwa::NokhwaError),

    #[from]
    OpenCV(opencv::Error),

    NotInGitRepo,
    NoHomeDirectory,
    NoRepoName,
    GitCommandFailed,
    ConfigError { message: String },
    ModelDownloadError { message: String },
    ModelValidationError { message: String },
    CameraError { message: String, path: PathBuf },
}

impl std::fmt::Display for Error {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}

impl std::error::Error for Error {}
