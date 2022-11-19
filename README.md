# BookBeat
This is an unofficial BookBeat CLI and library.

## Usage
```
Usage: bookbeat [OPTION]... --output [FOLDER]

Options:
 --username [NAME]      Username or E-Mail address
 --password [PASSWORD]  Password
 --force-fetch          Overwrite token cache
 --sfw                  Exclude explicit results
 --ebook [boolean]      Download ebooks (Default: true)
 --audiobook [boolean]  Download audio books (Default: true)

Variable count options:
 --id [ID]              Bookbeat ID
 --audioisbn [ISBN]     International Standard Book Number (Audiobook)
 --ebookisbn [ISBN]     International Standard Book Number (Ebook)
 --author [NAME]        Author Name
 --series [ID]          Series ID
 --language [LANG]      Language Name (Default: English)
```

## Rate limit
Sadly the API for licensing reports wrong stats.

```
x-rate-limit-limit: 1d
x-rate-limit-remaining: 199
x-rate-limit-reset: 2022-11-05T14:25:48.3259713Z

{"Message":"limit exceeded"}
```

It appears that you'll be able to download 200 e-books/audiobooks per month. After that every three days you'll get three more downloads.

## Tracing
When using the `mitm` feature flag, the client will try to proxy all traffic through `http://127.0.0.1:8888` and load a trusted certificate, `cert.pem` out of the current working directory.
