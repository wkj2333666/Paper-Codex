use anyhow::Result;
use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum RangeError {
    #[error("invalid byte range")]
    Invalid,
    #[error("byte range is not satisfiable")]
    Unsatisfiable,
}

pub fn parse_single_range(value: &str, length: u64) -> Result<Option<ByteRange>, RangeError> {
    if length == 0 {
        return Err(RangeError::Unsatisfiable);
    }
    let value = value.strip_prefix("bytes=").ok_or(RangeError::Invalid)?;
    if value.contains(',') {
        return Err(RangeError::Invalid);
    }
    let (start, end) = value.split_once('-').ok_or(RangeError::Invalid)?;
    if start.is_empty() {
        let suffix = end.parse::<u64>().map_err(|_| RangeError::Invalid)?;
        if suffix == 0 {
            return Err(RangeError::Unsatisfiable);
        }
        let start = length.saturating_sub(suffix.min(length));
        return Ok(Some(ByteRange {
            start,
            end: length - 1,
        }));
    }
    let start = start.parse::<u64>().map_err(|_| RangeError::Invalid)?;
    if start >= length {
        return Err(RangeError::Unsatisfiable);
    }
    let end = if end.is_empty() {
        length - 1
    } else {
        end.parse::<u64>().map_err(|_| RangeError::Invalid)?
    };
    if end < start {
        return Err(RangeError::Unsatisfiable);
    }
    Ok(Some(ByteRange {
        start,
        end: end.min(length - 1),
    }))
}

pub async fn pdf_response(
    path: &std::path::Path,
    revision: &str,
    request_headers: &HeaderMap,
) -> Result<Response> {
    let length = tokio::fs::metadata(path).await?.len();
    let etag = format!("\"{revision}\"");
    if request_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|item| item.trim() == etag))
    {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        apply_common_headers(response.headers_mut(), &etag);
        return Ok(response);
    }

    let requested = request_headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());
    let range = match requested.map(|value| parse_single_range(value, length)) {
        Some(Ok(range)) => range,
        Some(Err(_)) => {
            let mut response = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
            response.headers_mut().insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes */{length}"))?,
            );
            apply_common_headers(response.headers_mut(), &etag);
            return Ok(response);
        }
        None => None,
    };

    let mut file = tokio::fs::File::open(path).await?;
    let (status, body, content_length, content_range) = if let Some(range) = range {
        file.seek(std::io::SeekFrom::Start(range.start)).await?;
        let count = range.end - range.start + 1;
        (
            StatusCode::PARTIAL_CONTENT,
            Body::from_stream(ReaderStream::new(file.take(count))),
            count,
            Some(format!("bytes {}-{}/{length}", range.start, range.end)),
        )
    } else {
        (
            StatusCode::OK,
            Body::from_stream(ReaderStream::new(file)),
            length,
            None,
        )
    };
    let mut response = (status, body).into_response();
    apply_common_headers(response.headers_mut(), &etag);
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&content_length.to_string())?,
    );
    if let Some(content_range) = content_range {
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&content_range)?,
        );
    }
    Ok(response)
}

fn apply_common_headers(headers: &mut HeaderMap, etag: &str) {
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );
    if let Ok(value) = HeaderValue::from_str(etag) {
        headers.insert(header::ETAG, value);
    }
}
