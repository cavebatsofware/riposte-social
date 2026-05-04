/*  This file is part of riposte-social
 *  Copyright (C) 2026 Grant DeFayette
 *
 *  riposte-social is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, version 3 of the License (GPL-3.0-only).
 *
 *  riposte-social is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with riposte-social.  If not, see <https://www.gnu.org/licenses/gpl-3.0.html>.
 */
//! Mock S3 client scaffolding for tests. Symmetrical to `ses_mock.rs`.

use std::sync::{Arc, Mutex};

use aws_sdk_s3::operation::get_object::{GetObjectInput, GetObjectOutput};
use aws_sdk_s3::operation::put_object::{PutObjectInput, PutObjectOutput};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client as S3Client;
use aws_smithy_mocks::{mock, mock_client, RuleMode};
use riposte_social::s3::S3Service;

pub const TEST_BUCKET: &str = "test-bucket";

/// A single captured `PutObject` call.
#[derive(Debug, Clone)]
pub struct CapturedUpload {
    pub bucket: String,
    pub key: String,
    pub content_type: String,
    pub body: Vec<u8>,
}

/// A single captured `GetObject` call.
#[derive(Debug, Clone)]
pub struct CapturedDownload {
    pub bucket: String,
    pub key: String,
}

/// Shared state collected by the mocks.
#[derive(Clone, Default)]
pub struct S3Spy {
    uploads: Arc<Mutex<Vec<CapturedUpload>>>,
    downloads: Arc<Mutex<Vec<CapturedDownload>>>,
}

impl S3Spy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn uploads(&self) -> Vec<CapturedUpload> {
        self.uploads.lock().unwrap().clone()
    }

    pub fn downloads(&self) -> Vec<CapturedDownload> {
        self.downloads.lock().unwrap().clone()
    }

    fn push_upload(&self, call: CapturedUpload) {
        self.uploads.lock().unwrap().push(call);
    }

    fn push_download(&self, call: CapturedDownload) {
        self.downloads.lock().unwrap().push(call);
    }
}

fn extract_put(input: &PutObjectInput) -> CapturedUpload {
    let bucket = input.bucket().unwrap_or_default().to_string();
    let key = input.key().unwrap_or_default().to_string();
    let content_type = input.content_type().unwrap_or_default().to_string();
    // `ByteStream::bytes()` returns `Some(&[u8])` when the body is already
    // in memory, which is always the case in production (`data.into()` from
    // `Vec<u8>` in `src/s3.rs::upload_file`).
    let body = input.body().bytes().map(|b| b.to_vec()).unwrap_or_default();
    CapturedUpload {
        bucket,
        key,
        content_type,
        body,
    }
}

fn extract_get(input: &GetObjectInput) -> CapturedDownload {
    CapturedDownload {
        bucket: input.bucket().unwrap_or_default().to_string(),
        key: input.key().unwrap_or_default().to_string(),
    }
}

/// Build a mocked S3 client where every `GetObject` returns `body` and every
/// `PutObject` captures into the spy and returns success. Use this when a
/// test exercises both the download and upload paths.
pub fn mock_s3_get_ok_put_ok(body: Vec<u8>, spy: &S3Spy) -> S3Client {
    let spy_get = spy.clone();
    let get_rule =
        mock!(S3Client::get_object).then_compute_output(move |input: &GetObjectInput| {
            spy_get.push_download(extract_get(input));
            GetObjectOutput::builder()
                .body(ByteStream::from(body.clone()))
                .build()
        });

    let spy_put = spy.clone();
    let put_rule =
        mock!(S3Client::put_object).then_compute_output(move |input: &PutObjectInput| {
            spy_put.push_upload(extract_put(input));
            PutObjectOutput::builder().build()
        });

    mock_client!(aws_sdk_s3, RuleMode::MatchAny, [&get_rule, &put_rule])
}

/// Build a mocked S3 client where `GetObject` returns `body`.
pub fn mock_s3_get_ok(body: Vec<u8>, spy: &S3Spy) -> S3Client {
    let spy_clone = spy.clone();
    let rule = mock!(S3Client::get_object).then_compute_output(move |input: &GetObjectInput| {
        spy_clone.push_download(extract_get(input));
        GetObjectOutput::builder()
            .body(ByteStream::from(body.clone()))
            .build()
    });

    mock_client!(aws_sdk_s3, RuleMode::MatchAny, [&rule])
}

/// Build a mocked S3 client where `GetObject` always returns `NoSuchKey`.
pub fn mock_s3_get_not_found() -> S3Client {
    use aws_sdk_s3::operation::get_object::GetObjectError;
    use aws_sdk_s3::types::error::NoSuchKey;

    let rule = mock!(S3Client::get_object)
        .then_error(|| GetObjectError::NoSuchKey(NoSuchKey::builder().build()));

    mock_client!(aws_sdk_s3, RuleMode::Sequential, [&rule])
}

/// Build a mocked S3 client where `PutObject` captures each call into the spy
/// and returns success. Uses `MatchAny` so the rule is reused across multiple
/// PUT calls (e.g. upload of both index.html and Document.docx).
pub fn mock_s3_put_ok(spy: &S3Spy) -> S3Client {
    let spy_clone = spy.clone();
    let rule = mock!(S3Client::put_object).then_compute_output(move |input: &PutObjectInput| {
        spy_clone.push_upload(extract_put(input));
        PutObjectOutput::builder().build()
    });

    mock_client!(aws_sdk_s3, RuleMode::MatchAny, [&rule])
}

/// Build a mocked S3 client where `PutObject` always fails.
pub fn mock_s3_put_err() -> S3Client {
    use aws_sdk_s3::operation::put_object::PutObjectError;
    use aws_smithy_types::error::ErrorMetadata;

    let rule = mock!(S3Client::put_object).then_error(|| {
        PutObjectError::generic(
            ErrorMetadata::builder()
                .code("InternalError")
                .message("mock internal error")
                .build(),
        )
    });

    mock_client!(aws_sdk_s3, RuleMode::Sequential, [&rule])
}

/// Build a mocked S3 client that accepts PutObject and DeleteObject.
/// Suitable as a default for tests that don't need specific S3 behavior.
pub fn mock_s3_default(spy: &S3Spy) -> S3Client {
    use aws_sdk_s3::operation::delete_object::{DeleteObjectInput, DeleteObjectOutput};

    let spy_put = spy.clone();
    let put_rule =
        mock!(S3Client::put_object).then_compute_output(move |input: &PutObjectInput| {
            spy_put.push_upload(extract_put(input));
            PutObjectOutput::builder().build()
        });

    let delete_rule =
        mock!(S3Client::delete_object).then_compute_output(move |_input: &DeleteObjectInput| {
            DeleteObjectOutput::builder().build()
        });

    mock_client!(aws_sdk_s3, RuleMode::MatchAny, [&put_rule, &delete_rule])
}

/// Convenience: wrap any mock client into an `S3Service`.
pub fn build_test_s3_service(client: S3Client) -> S3Service {
    S3Service::with_client(client, TEST_BUCKET.to_string())
}
