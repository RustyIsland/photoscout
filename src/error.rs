use thiserror::Error;

#[derive(Debug, Error)]
pub enum PhotoScoutError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),

    #[error("image decoding error: {0}")]
    Image(#[from] image::ImageError),

    #[error("path has no file name: {0}")]
    MissingFileName(String),
}
