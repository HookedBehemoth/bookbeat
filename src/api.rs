#[derive(Debug)]
pub enum Error {
    Api(u16, String),
    Cdn(u16, String),
    Status(String),
    Reqwest(reqwest::Error),
    Serde(serde_json::Error),
}

impl Error {
    pub fn from_reqwest(error: reqwest::Error) -> Self {
        Self::Reqwest(error)
    }
    pub fn from_serde(error: serde_json::Error) -> Self {
        Self::Serde(error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
