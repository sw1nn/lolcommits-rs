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

    NotInGitRepo,
    NoHomeDirectory,
    NoRepoName,
}
