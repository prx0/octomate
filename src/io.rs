use crate::error::Error;
use std::path::Path;
use tokio::fs;

pub async fn read_file(path: impl AsRef<Path>) -> Result<Vec<u8>, Error> {
    let bytes = fs::read(&path).await?;
    Ok(bytes)
}
