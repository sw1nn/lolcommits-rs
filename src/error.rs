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

    #[from]
    Xdg(xdg::BaseDirectoriesError),

    #[from]
    TomlDeserialize(toml::de::Error),

    #[from]
    TomlSerialize(toml::ser::Error),

    #[from]
    Reqwest(reqwest::Error),

    NotInGitRepo,
    NoHomeDirectory,
    NoRepoName,
    GitCommandFailed,

    ConfigFileRead {
        path: PathBuf,
        source: std::io::Error,
    },
    ConfigFileWrite {
        path: PathBuf,
        source: std::io::Error,
    },

    HttpError {
        status: u16,
    },

    ModelFileTooSmall {
        size: usize,
    },
    ModelChecksumMismatch {
        expected: String,
        actual: String,
    },
    ModelDirectoryCreate {
        path: PathBuf,
        source: std::io::Error,
    },
    ModelFileWrite {
        path: PathBuf,
        source: std::io::Error,
    },

    CameraSymlinkResolution {
        path: PathBuf,
        source: std::io::Error,
    },
    CameraInvalidDevicePath {
        path: PathBuf,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}

impl std::error::Error for Error {}
