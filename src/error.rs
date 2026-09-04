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

    #[from]
    PngEncoding(png::EncodingError),

    #[from]
    PngDecoding(png::DecodingError),

    #[from]
    SerdeJson(serde_json::Error),

    #[from]
    Jwt(jsonwebtoken::errors::Error),

    #[from]
    Keyring(keyring::Error),

    NotInGitRepo,
    NoHomeDirectory,
    NoRepoName,
    GitCommandFailed,

    ConfigFileNotFound {
        path: PathBuf,
    },
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
    /// The configured device path does not exist, so the device cannot be a
    /// camera on this machine. Other configured devices are still tried.
    CameraDeviceNotFound {
        path: PathBuf,
    },
    /// Every configured device was missing or unusable as a path, so there was
    /// nothing to capture from.
    NoCameraDeviceAvailable {
        devices: Vec<String>,
    },
    CameraBusy {
        device: String,
    },

    ServerConnectionFailed {
        url: String,
        source: reqwest::Error,
    },

    UploadFailed {
        status: u16,
        body: String,
    },

    InvalidUploadField {
        field: &'static str,
    },
    PathTraversal {
        name: String,
    },

    // Access token verification (daemon side).
    JwksFetch {
        url: String,
        source: reqwest::Error,
    },
    JwksUnavailable {
        url: String,
        status: u16,
    },
    /// The token's `kid` is in neither the cached key set nor a fresh fetch.
    UnknownSigningKey,
    TokenMissingKeyId,
    MissingBearerToken,
    /// Minted for a different OIDC client, so not usable at this service.
    WrongClientId,
    MissingRequiredGroup {
        group: String,
    },

    // Device authorization grant (CLI side).
    DeviceAuthorizationFailed {
        status: u16,
        body: String,
    },
    DeviceCodeExpired,
    DeviceAuthorizationDenied,
    TokenRequestFailed {
        status: u16,
        body: String,
    },
    /// An RFC 6749 error response from the token endpoint.
    TokenEndpointError {
        error: String,
        description: Option<String>,
    },
    /// No stored credentials, or the refresh token is no longer accepted.
    NotLoggedIn,
    TokenStoreWrite {
        path: PathBuf,
        source: std::io::Error,
    },

    UnknownCameraFormat {
        format: String,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}

impl std::error::Error for Error {}
