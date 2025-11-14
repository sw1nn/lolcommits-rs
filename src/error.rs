use derive_more::{Display, Error, From};

pub type Result<T> = std::result::Result<T, LolcommitsError>;

#[derive(Debug, Display, Error, From)]
pub enum LolcommitsError {
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

    #[display("Not in a git repository")]
    NotInGitRepo,

    #[display("Could not determine home directory")]
    NoHomeDirectory,

    #[display("Could not determine repository name")]
    NoRepoName,

    #[display("Git command failed")]
    GitCommandFailed,

    #[display("Configuration error: {message}")]
    ConfigError { message: String },
}
