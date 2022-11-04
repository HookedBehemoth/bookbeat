mod api;
mod client;

use std::{fs, path::PathBuf, str::FromStr};

use chrono::Datelike;
use futures_util::stream::StreamExt;
use tokio::fs::File;
use tokio::io::{stdin, AsyncReadExt, AsyncWriteExt};

use crate::client::{AuthToken, Client, BookFormat};

const TOKEN_PATH: &str = "token.json";
const USAGE: &str = "Usage: bookbeat [OPTION]... --output [FOLDER]

Options:
 --username [NAME]      Username or E-Mail address
 --password [PASSWORD]  Password
 --force-fetch          Overwrite token cache
 --sfw                  Exclude explicit results
 --ebook [boolean]      Download ebooks (Default: false)
 --audiobook [boolean]  Download audio books (Default: true)
 --market [MARKET]      Target market (Default: Germany)

Variable count options:
 --id [ID]              Bookbeat ID
 --isbn [ISBN]          International Standard Book Number
 --author [NAME]        Author Name
 --series [ID]          Series ID
 --language [LANG]      Language Name (Default: English)";
const PROGRESS_TEMPLATE: &str = "{wide_bar} [{bytes:10}/{total_bytes:10}] {eta:4}";

#[tokio::main]
async fn main() -> api::Result<()> {
    let mut args = pico_args::Arguments::from_env();

    if args.contains("--help") {
        eprintln!("{}", USAGE);
        return Ok(());
    }

    /* Load ouput directory, fallback to cwd */
    let dest = if let Ok(path) = args.value_from_str::<&str, String>("--output") {
        PathBuf::from_str(&path).unwrap()
    } else {
        eprintln!("Storing in current working directory");
        std::env::current_dir().unwrap()
    };

    /* Allow overriding token */
    if args.contains("--force-fetch") {
        let _ = fs::remove_file(TOKEN_PATH);
    }

    let sfw = args.contains("--sfw");
    let ebook = args.value_from_str("--ebook").unwrap_or(false);
    let audiobook = args.value_from_str("--audiobook").unwrap_or(true);
    let market = args
        .value_from_str("--market")
        .unwrap_or_else(|_| "Germany".to_owned());

    let mut languages: Vec<String> = args.values_from_str("--language").unwrap();
    if languages.is_empty() {
        languages.push("English".to_owned());
    }
    let languages: Vec<&str> = languages.iter().map(|s| s.as_str()).collect();

    let client = if let Ok(token) = fs::read_to_string(TOKEN_PATH) {
        let token: AuthToken = serde_json::from_str(&token).unwrap();
        let client = Client::from_token(token).await?;

        write_token(client.extract_token()).await;

        client
    } else {
        let username: String = args.value_from_str("--username").unwrap();
        let password: String = args.value_from_str("--password").unwrap();

        let client = Client::login(&username, &password).await?;

        write_token(client.extract_token()).await;

        client
    };

    let user = client.users().await?;

    if !user.subscribed() {
        eprintln!("WARNING: Not subscribed. There will be dragons.");
        if !confirm("Continue?").await {
            return Ok(());
        }
    }

    let downloader = Downloader::new(dest);

    while let Ok(Some(id)) = args.opt_value_from_str::<&str, u32>("--id") {
        let book = client.books(&market, id).await?;

        for edition in book.editions {
            let isbn = &edition.isbn;
            match edition.format {
                BookFormat::AudioBook => if audiobook {
                    let file_name = format!("{} ({}).m4a", &book.title, isbn);
    
                    let _ = downloader.download(&client, isbn, &file_name).await?;
                }
                BookFormat::EBook => if ebook {
                    let file_name = format!("{} ({}).epub", &book.title, isbn);
    
                    let _ = downloader.download(&client, isbn, &file_name).await?;
                }
            }
        }
    }

    while let Ok(Some(name)) = args.opt_value_from_str::<&str, String>("--author") {
        println!("Downloading author \"{}\"", name);

        download_search(
            &client,
            Some(&name),
            None,
            sfw,
            audiobook,
            ebook,
            &languages,
            &downloader,
        )
        .await?;
    }
    while let Ok(Some(name)) = args.opt_value_from_str::<&str, String>("--narrator") {
        println!("Downloading narrator \"{}\"", name);

        download_search(
            &client,
            None,
            Some(&name),
            sfw,
            audiobook,
            ebook,
            &languages,
            &downloader,
        )
        .await?;
    }
    while let Ok(Some(id)) = args.opt_value_from_str::<&str, u32>("--series") {
        let mut offset: usize = 0;
        const STEP: usize = 50;
        loop {
            let series = client.series(id, offset, STEP).await?;

            println!(
                "Downloading \"{}\" ({}/{})",
                series.name, offset, series.count
            );

            let parts = series._embedded.parts;

            for part in &parts {
                let prefix: Option<String> = part.partnumber.map(|index| format!("{:03} ", &index));
                let prefix = prefix.as_deref().unwrap_or("");
                let book = &part._embedded.book;

                if audiobook {
                    if let Some(isbn) = &book.audiobookisbn {
                        let file_name = format!("{}{} ({}).m4a", prefix, &book.title, isbn);

                        let path = downloader.download(&client, isbn, &file_name).await?;

                        set_m4a_metadata(&path, book).await;
                    }
                }

                if ebook {
                    if let Some(isbn) = &book.ebookisbn {
                        let file_name = format!("{}{} ({}).epub", prefix, &book.title, isbn);

                        let _ = downloader.download(&client, isbn, &file_name).await?;
                    }
                }
            }

            if parts.len() < STEP {
                break;
            }

            offset += STEP;
        }
    }

    while let Ok(Some(isbn)) = args.opt_value_from_str::<&str, String>("--audioisbn") {
        let file_name = format!("{isbn}.m4a");
        downloader.download(&client, &isbn, &file_name).await?;
    }
    while let Ok(Some(isbn)) = args.opt_value_from_str::<&str, String>("--ebookisbn") {
        let file_name = format!("{isbn}.epub");
        downloader.download(&client, &isbn, &file_name).await?;
    }

    Ok(())
}

async fn download_search(
    client: &Client,
    author: Option<&str>,
    narrator: Option<&str>,
    sfw: bool,
    audiobook: bool,
    ebook: bool,
    languages: &[&str],
    downloader: &Downloader,
) -> api::Result<()> {
    let mut offset: usize = 0;
    const STEP: usize = 50;
    loop {
        let search = client
            .search(author, narrator, offset, STEP, languages, !sfw)
            .await?;

        let books = search._embedded.books;

        for book in &books {
            if audiobook {
                if let Some(isbn) = &book.audiobookisbn {
                    let file_name = format!("{} ({}).m4a", &book.title, isbn);

                    let path = downloader.download(client, isbn, &file_name).await?;

                    set_m4a_metadata(&path, book).await;
                }
            }

            if ebook {
                if let Some(isbn) = &book.ebookisbn {
                    let file_name = format!("{} ({}).epub", &book.title, isbn);

                    let _ = downloader.download(client, isbn, &file_name).await?;
                }
            }
        }

        if books.len() < STEP {
            break;
        }

        offset += STEP;
    }

    Ok(())
}

async fn confirm(message: &str) -> bool {
    eprintln!("{} [y/N]", message);
    let answer = stdin().read_u8().await.expect("Aborted");
    matches!(answer, b'y' | b'Y')
}

async fn write_token(token: &client::AuthToken) {
    let token = serde_json::to_vec(token).unwrap();
    std::fs::write(TOKEN_PATH, &token).unwrap();
}

struct Downloader {
    path: PathBuf,
    client: reqwest::Client,
    style: indicatif::ProgressStyle,
}

impl Downloader {
    fn new(path: PathBuf) -> Self {
        /* 15 Minute keepalive */
        let keepalive = std::time::Duration::from_secs(15 * 60);

        let client = reqwest::ClientBuilder::new()
            .user_agent("okhttp/4.10.0")
            .tcp_keepalive(keepalive)
            .build()
            .unwrap();

        let style = indicatif::ProgressStyle::default_bar()
            .template(PROGRESS_TEMPLATE)
            .unwrap();

        Self {
            path,
            client,
            style,
        }
    }

    async fn download(&self, client: &Client, isbn: &str, file_name: &str) -> api::Result<PathBuf> {
        /* Request link */
        let license = client.license(isbn).await?;

        let url = license._links.download.unwrap();

        let mut path = self.path.clone();
        path.push(file_name.replace('/', "_"));

        let mut file = File::create(&path).await.unwrap();

        let response = self
            .client
            .get(url.href)
            .send()
            .await
            .map_err(api::Error::from_reqwest)?;

        let status = response.status();
        if !status.is_success() {
            let error = response
                .text()
                .await
                .unwrap_or_else(|_| "(Unknown)".to_owned());
            return Err(api::Error::Cdn(status.as_u16(), error));
        }

        let bar =
            indicatif::ProgressBar::new(license.filesize as u64).with_style(self.style.clone());

        let mut stream = response.bytes_stream();
        while let Some(Ok(mut chunk)) = stream.next().await {
            bar.inc(chunk.len() as u64);
            let _ = file.write_all_buf(&mut chunk).await;
        }

        bar.finish_and_clear();

        Ok(path)
    }
}

async fn set_m4a_metadata(path: &PathBuf, book: &client::SearchBook) {
    let mut tag = mp4ameta::Tag::read_from_path(path).unwrap();

    tag.set_title(&book.title);
    tag.set_year(book.published.year().to_string());
    tag.set_album(&book.title);
    tag.set_artist(&book.author);
    tag.set_album_artist(&book.author);

    let Some(image) = &book.image else {
        eprintln!("No image set");
        return;
    };

    let Ok(response) = reqwest::get(image).await else{
        eprintln!("Failed to download cover");
        return;
    };

    let format = match response.headers().get("content-type").map(|v| v.to_str()) {
        Some(Ok("image/jpeg")) => mp4ameta::ImgFmt::Jpeg,
        Some(Ok("image/png")) => mp4ameta::ImgFmt::Png,
        Some(Ok(format)) => {
            eprintln!("Unknown image format {format}");
            return;
        }
        _ => {
            eprintln!("Unknown image format (undefined)");
            return;
        }
    };

    let bytes = response.bytes().await.unwrap();
    let img = mp4ameta::Img::new(format, bytes);
    tag.add_artwork(img);

    tag.write_to_path(path).unwrap();
}
