use crate::api::{Error, Result};

use reqwest::{
    header::{HeaderMap, HeaderValue},
    Response,
};

type DateTime = chrono::DateTime<chrono::Utc>;

const USER_AGENT: &str = "BookBeat 9.7.1 phone OnePlus Dalvik/2.1.0 (Linux; U; Android 10; ONEPLUS A5000 Build/QKQ1.191014.012)";
const API_STATUS: &str = "https://status.bookbeat.com/api/prod/status/";
const LOGIN_URL: &str = "https://api.bookbeat.com/api/login";
const REFRESH_URL: &str = "https://api.bookbeat.com/api/login/refresh";
const USERS_URL: &str = "https://api.bookbeat.com/api/users";
const TABSEARCH_BOOKS_URL: &str = "https://search-api.bookbeat.com/api/tabsearch/books";
const SEARCH_BOOKS_URL: &str = "https://api.bookbeat.com/api/search/books";

#[derive(serde::Deserialize, Debug)]
struct Status {
    #[serde(rename = "type")]
    typ: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct Link {
    pub href: String,
}

#[derive(serde::Serialize)]
struct LoginRequest<'a> {
    username: &'a str,
    password: &'a str, // Year of our lord 2000 + 22
}

#[derive(serde::Serialize)]
struct RefreshRequest<'a> {
    refreshtoken: &'a str,
}

#[derive(serde::Deserialize, Debug)]
struct Login {
    refreshtoken: String,
    token: String,
    expiresin: i64,
}

impl Login {
    fn into_auth_token(self) -> AuthToken {
        let token = format!("Bearer {}", self.token);
        let expiration = chrono::Utc::now() + chrono::Duration::seconds(self.expiresin);
        AuthToken {
            refreshtoken: self.refreshtoken,
            token,
            expiration,
        }
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct User {
    pub email: String,
    pub userid: u64,
    pub firstname: String,
    pub lastname: String,
    pub displayname: String,
    pub market: String,
    pub iskid: bool,
    _embedded: BookBeatSubscriptionInfoEmbedded,
}

#[derive(serde::Deserialize, Debug)]
struct BookBeatSubscriptionInfoEmbedded {
    subscriptioninfo: BookBeatSubscriptionInfo,
}

#[derive(serde::Deserialize, Debug)]
struct BookBeatSubscriptionInfo {
    validsubscription: bool,
}

impl User {
    pub fn subscribed(&self) -> bool {
        self._embedded.subscriptioninfo.validsubscription
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct BookBeatTabSearch {
    pub count: usize,
    pub books: Search,
}

#[derive(serde::Deserialize, Debug)]
pub struct Search {
    pub count: usize,
    pub _embedded: SearchEmbedded,
}

#[derive(serde::Deserialize, Debug)]
pub struct SearchEmbedded {
    pub books: Vec<SearchBook>,
}

#[derive(serde::Deserialize, Debug)]
pub struct SearchBook {
    pub id: usize,
    pub title: String,
    pub image: Option<String>,
    pub author: String,
    pub grade: f32,
    pub language: String,
    pub audiobookisbn: Option<String>,
    pub ebookisbn: Option<String>,
    pub published: DateTime,
}

#[derive(serde::Deserialize, Debug)]
pub struct Book {
    pub id: usize,
    pub title: String,
    pub author: String,
    pub summary: String,
    pub grade: f32,
    pub cover: String,
    pub narrator: String,
    pub language: String,
    pub published: DateTime,
    pub genres: Vec<Genres>,
    pub editions: Vec<Edition>,
}

#[derive(serde::Deserialize, Debug)]
pub struct Genres {
    pub genreid: u32,
    pub name: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct Edition {
    pub id: u32,
    pub isbn: String,
    pub format: BookFormat,
    pub published: DateTime,
    pub publisher: String,
}

#[derive(serde::Deserialize, Debug)]
pub enum BookFormat {
    #[serde(rename = "audioBook")]
    AudioBook,
    #[serde(rename = "eBook")]
    EBook,
}

#[derive(serde::Deserialize, Debug)]
pub struct Series {
    pub count: usize,
    pub id: u32,
    pub name: String,
    pub description: Option<String>,
    pub _embedded: SeriesEmbedded,
}

#[derive(serde::Deserialize, Debug)]
pub struct SeriesEmbedded {
    pub parts: Vec<SeriesPart>,
}

#[derive(serde::Deserialize, Debug)]
pub struct SeriesPart {
    pub partnumber: Option<u32>,
    pub _embedded: SeriesPartEmbedded,
}

#[derive(serde::Deserialize, Debug)]
pub struct SeriesPartEmbedded {
    pub book: SearchBook,
}

#[derive(serde::Deserialize, Debug)]
pub struct License {
    pub isbn: String,
    pub assetid: String,
    pub source: String,
    pub filesize: usize,
    pub tracks: Vec<Track>,
    pub _links: LicenseLinks,
}

#[derive(serde::Deserialize, Debug)]
pub struct Track {
    pub start: usize,
    pub end: usize,
}

#[derive(serde::Deserialize, Debug)]
pub struct LicenseLinks {
    pub download: Option<Link>,
    pub stream: Option<Link>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct AuthToken {
    refreshtoken: String,
    token: String,
    expiration: DateTime,
}

#[derive(serde::Deserialize)]
pub struct ApiError {
    #[serde(rename = "Message")]
    message: String,
}

pub struct Client {
    client: reqwest::Client,
    token: AuthToken,
}

impl Client {
    fn inner() -> Result<reqwest::Client> {
        let headers = [
            ("api-version", "9"),
            (
                "bb-device",
                "4ac2d433-9126-4635-a769-553319a650c1 T05FUExVUyBPTkVQTFVTIEE1MDAw",
            ),
            ("bb-client", "BookBeatApp"),
            ("bb-market", "Germany"),
            ("accept-language", "en-US"),
        ];

        let headers = headers.iter().fold(
            reqwest::header::HeaderMap::new(),
            |mut map, &(k, v)| -> HeaderMap {
                map.append(k, HeaderValue::from_static(v));
                map
            },
        );

        /* 15 Minute keepalive */
        let keepalive = std::time::Duration::from_secs(15 * 60);

        let mut builder = reqwest::ClientBuilder::new()
            .user_agent(USER_AGENT)
            .tcp_keepalive(keepalive)
            .default_headers(headers);

        if cfg!(debug_assertions) {
            println!("Installing ssl proxy with certificate");
            let proxy = reqwest::Proxy::all("http://127.0.0.1:8888").unwrap();
            let pem = include_bytes!("../cert.pem");
            let cert = reqwest::Certificate::from_pem(pem).unwrap();
            builder = builder.proxy(proxy).add_root_certificate(cert);
        }

        let inner = builder.build().map_err(Error::from_reqwest)?;

        Ok(inner)
    }

    pub async fn login(username: &str, password: &str) -> Result<Self> {
        let client = Self::inner()?;

        let status = Self::status(&client).await?;

        if status != "OK" {
            return Err(Error::Status(status));
        }

        let request = LoginRequest { username, password };
        let body = serde_json::to_vec(&request).map_err(Error::from_serde)?;
        let response: Response = client
            .post(LOGIN_URL)
            .body(body)
            .header("content-type", "application/json; charset=UTF-8")
            .header("accept", "application/hal+json")
            .send()
            .await
            .map_err(Error::from_reqwest)?;

        let login: Login = Self::parse(response).await?;

        let token = login.into_auth_token();

        Ok(Self { client, token })
    }

    async fn refresh_token(&mut self) -> Result<()> {
        let body = RefreshRequest {
            refreshtoken: &self.token.refreshtoken,
        };

        let login: Login = self.post_with_auth(REFRESH_URL, &body).await?;

        self.token = login.into_auth_token();

        Ok(())
    }

    pub async fn from_token(token: AuthToken) -> Result<Self> {
        let client = Self::inner()?;

        let mut client = Self { client, token };

        if client.token.expiration < chrono::Utc::now() {
            client.refresh_token().await?;
        }

        Ok(client)
    }

    pub fn extract_token(&self) -> &'_ AuthToken {
        &self.token
    }

    async fn parse<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> Result<T> {
        response.json().await.map_err(Error::from_reqwest)
    }

    async fn post_with_auth<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<R> {
        let token = &self.token.token;
        let body = serde_json::to_vec(body).map_err(Error::from_serde)?;
        let response = self
            .client
            .post(url)
            .body(body)
            .header("content-type", "application/json; charset=UTF-8")
            .header("accept", "application/hal+json")
            .header("authorization", token)
            .send()
            .await
            .map_err(Error::from_reqwest)?;

        let status = response.status();
        if !status.is_success() {
            let error: ApiError = Self::parse(response).await?;
            return Err(Error::Api(status.as_u16(), error.message));
        }

        Self::parse(response).await
    }

    async fn get_with_auth<R: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        query: Option<&[(&str, &str)]>,
    ) -> Result<R> {
        let mut request = self
            .client
            .get(url)
            .header("authorization", &self.token.token)
            .header("accept", "application/hal+json");

        if let Some(query) = query {
            request = request.query(query);
        }

        let response = request.send().await.map_err(Error::from_reqwest)?;

        let status = response.status();
        if !status.is_success() {
            let error: ApiError = Self::parse(response).await?;
            return Err(Error::Api(status.as_u16(), error.message));
        }

        Self::parse(response).await
    }

    async fn status(client: &reqwest::Client) -> Result<String> {
        let response = client
            .get(API_STATUS)
            .send()
            .await
            .map_err(Error::from_reqwest)?;
        let status: Status = response.json().await.map_err(Error::from_reqwest)?;

        Ok(status.typ)
    }

    pub async fn users(&self) -> Result<User> {
        self.get_with_auth(USERS_URL, None).await
    }

    pub async fn tabsearch_books(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
        language: &[&str],
        market: &str,
        kid: bool,
        includeerotic: bool,
    ) -> Result<Search> {
        let offset = offset.to_string();
        let limit = limit.to_string();
        let kid = if kid { "true" } else { "false" };
        let includeerotic = if includeerotic { "true" } else { "false" };
        let mut query = vec![
            ("query", query),
            ("offset", &offset),
            ("limit", &limit),
            ("market", market),
            ("kid", kid),
            ("includeerotic", includeerotic),
        ];
        for &language in language {
            query.push(("language", language));
        }
        self.get_with_auth(TABSEARCH_BOOKS_URL, Some(&query)).await
    }

    pub async fn search(
        &self,
        author: Option<&str>,
        narrator: Option<&str>,
        offset: usize,
        limit: usize,
        language: &[&str],
        includeerotic: bool,
    ) -> Result<Search> {
        let offset = offset.to_string();
        let limit = limit.to_string();
        let includeerotic = if includeerotic { "true" } else { "false" };
        let mut query: Vec<(&str, &str)> = vec![
            ("offset", &offset),
            ("limit", &limit),
            ("sortby", "publishdate"),
            ("sortby", "publishdate"),
            ("includeerotic", includeerotic),
        ];

        if let Some(author) = author {
            query.push(("author", author));
        }

        if let Some(narrator) = narrator {
            query.push(("narrator", narrator));
        }

        for &language in language {
            query.push(("language", language));
        }

        self.get_with_auth(SEARCH_BOOKS_URL, Some(&query)).await
    }

    pub async fn books(&self, market: &str, id: u32) -> Result<Book> {
        let url = format!("https://api.bookbeat.com/api/books/{market}/{id}");
        self.get_with_auth(&url, None).await
    }

    pub async fn license(&self, isbn: &str) -> Result<License> {
        let url = format!("https://api.bookbeat.com/api/content/{isbn}/license");
        self.get_with_auth(&url, None).await
    }

    pub async fn series(&self, id: u32, offset: usize, limit: usize) -> Result<Series> {
        let url = format!("https://api.bookbeat.com/api/series/{id}");
        let offset = offset.to_string();
        let limit = limit.to_string();
        let query: [(&str, &str); 2] = [("offset", &offset), ("limit", &limit)];
        self.get_with_auth(&url, Some(&query)).await
    }
}
