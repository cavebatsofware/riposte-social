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
use anyhow::Result;
use aws_sdk_s3::Client;
use std::env;
use std::error::Error as StdError;
use std::fmt::Display;

/// Render an AWS SDK error with its full cause chain so logs and admin UI
/// see the underlying problem (DNS resolution, TLS handshake failure,
/// service-side 4xx body, etc.) instead of the SDK's bland top-level
/// message ("dispatch failure", "service error", ...).
fn format_aws_error<E: Display + StdError + 'static>(e: &E) -> String {
    let mut msg = format!("{}", e);
    let mut source: Option<&dyn StdError> = e.source();
    while let Some(s) = source {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        source = s.source();
    }
    msg
}

#[derive(Clone)]
pub struct S3Service {
    client: Client,
    bucket_name: String,
}

impl S3Service {
    /// Construct an `S3Service` from a pre-built S3 client. Used by `new()`
    /// for production and by tests that inject a mocked client.
    pub fn with_client(client: Client, bucket_name: String) -> Self {
        Self {
            client,
            bucket_name,
        }
    }

    pub async fn new() -> Result<Self> {
        let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .load()
            .await;

        let mut s3_config = aws_sdk_s3::config::Builder::from(&aws_config);

        if let Ok(endpoint_url) = env::var("S3_ENDPOINT_URL") {
            if !endpoint_url.is_empty() {
                tracing::info!("Using custom S3 endpoint: {}", endpoint_url);
                s3_config = s3_config.endpoint_url(&endpoint_url);
            }
        }

        if let Ok(region) = env::var("S3_REGION") {
            if !region.is_empty() {
                tracing::info!("Using custom S3 region: {}", region);
                s3_config = s3_config.region(aws_sdk_s3::config::Region::new(region));
            }
        }

        if env::var("S3_FORCE_PATH_STYLE")
            .unwrap_or_default()
            .to_lowercase()
            == "true"
        {
            tracing::info!("Using path-style S3 addressing");
            s3_config = s3_config.force_path_style(true);
        }

        let client = Client::from_conf(s3_config.build());
        let bucket_name = env::var("S3_BUCKET_NAME")
            .unwrap_or_else(|_| "riposte-social-documents".to_string());

        Ok(Self::with_client(client, bucket_name))
    }

    /// Upload at an arbitrary S3 key with an explicit content-type. Used by
    /// callers that don't fit the `{prefix}/{filename}` shape (e.g. post
    /// media keys are `posts/{post_id}/{media_id}` with no extension), or
    /// when the mime type is known from the source's Content-Type header
    /// rather than the filename.
    pub async fn put_object_at(
        &self,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<()> {
        tracing::info!(
            "Uploading to S3: bucket={}, key={}, size={} bytes, type={}",
            self.bucket_name,
            key,
            data.len(),
            content_type
        );

        self.client
            .put_object()
            .bucket(&self.bucket_name)
            .key(key)
            .body(data.into())
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "S3 PutObject failed (bucket={}, key={}): {}",
                    self.bucket_name,
                    key,
                    format_aws_error(&e)
                )
            })?;

        Ok(())
    }

    /// Upload a local file at `path` to `key`. Used for archive uploads
    /// where holding the whole payload in memory is wasteful: the multipart
    /// handler streams the request body to a tempfile, then this method
    /// streams the tempfile to S3 without buffering it back into a Vec.
    pub async fn put_object_from_path(
        &self,
        key: &str,
        path: &std::path::Path,
        content_type: &str,
    ) -> Result<()> {
        tracing::info!(
            "Uploading file to S3: bucket={}, key={}, src={}, type={}",
            self.bucket_name,
            key,
            path.display(),
            content_type
        );

        let body = aws_sdk_s3::primitives::ByteStream::from_path(path)
            .await
            .map_err(|e| {
                anyhow::anyhow!("failed to open archive {} for upload: {}", path.display(), e)
            })?;

        self.client
            .put_object()
            .bucket(&self.bucket_name)
            .key(key)
            .body(body)
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| {
                // The SDK's top-level Display says "dispatch failure" or
                // similar without a cause; the real reason (DNS, TLS,
                // service error) is in the source chain. Walk it.
                anyhow::anyhow!(
                    "S3 PutObject failed (bucket={}, key={}): {}",
                    self.bucket_name,
                    key,
                    format_aws_error(&e)
                )
            })?;

        Ok(())
    }

    /// Fetch by full S3 key. Returns the bytes and the stored content-type
    /// (when S3 returned one), so the caller can set the right
    /// `Content-Type` response header without duplicating the type table.
    pub async fn get_object_at(&self, key: &str) -> Result<(Vec<u8>, Option<String>)> {
        tracing::debug!("Fetching from S3: bucket={}, key={}", self.bucket_name, key);

        let response = self
            .client
            .get_object()
            .bucket(&self.bucket_name)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "S3 GetObject failed (bucket={}, key={}): {}",
                    self.bucket_name,
                    key,
                    format_aws_error(&e)
                )
            })?;

        let content_type = response.content_type().map(|s| s.to_string());
        let data = response.body.collect().await?;
        let bytes = data.into_bytes().to_vec();
        Ok((bytes, content_type))
    }

    /// Delete by full S3 key.
    pub async fn delete_object_at(&self, key: &str) -> Result<()> {
        tracing::info!("Deleting from S3: bucket={}, key={}", self.bucket_name, key);
        self.client
            .delete_object()
            .bucket(&self.bucket_name)
            .key(key)
            .send()
            .await?;
        Ok(())
    }

    /// Fetch a file from S3 at path: {code}/{filename}
    /// For example: get_file("ABC123", "index.html") fetches s3://bucket/ABC123/index.html
    pub async fn get_file(&self, code: &str, filename: &str) -> Result<Vec<u8>> {
        let key = format!("{}/{}", code, filename);

        tracing::info!("Fetching from S3: bucket={}, key={}", self.bucket_name, key);

        let response = self
            .client
            .get_object()
            .bucket(&self.bucket_name)
            .key(&key)
            .send()
            .await?;

        let data = response.body.collect().await?;
        let bytes = data.into_bytes().to_vec();

        tracing::info!("Successfully fetched {} bytes from S3", bytes.len());
        Ok(bytes)
    }

    /// Upload a file to S3 at path: {code}/{filename}
    /// For example: upload_file("ABC123", "index.html", bytes) uploads to s3://bucket/ABC123/index.html
    pub async fn upload_file(&self, code: &str, filename: &str, data: Vec<u8>) -> Result<()> {
        let key = format!("{}/{}", code, filename);

        tracing::info!(
            "Uploading to S3: bucket={}, key={}, size={} bytes",
            self.bucket_name,
            key,
            data.len()
        );

        // Determine content type based on filename
        let content_type = match filename {
            f if f.ends_with(".html") => "text/html",
            f if f.ends_with(".pdf") => "application/pdf",
            f if f.ends_with(".docx") => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
            _ => "application/octet-stream",
        };

        self.client
            .put_object()
            .bucket(&self.bucket_name)
            .key(&key)
            .body(data.into())
            .content_type(content_type)
            .send()
            .await?;

        tracing::info!("Successfully uploaded {} to S3", key);
        Ok(())
    }

    /// Delete a file from S3 at path: {code}/{filename}
    pub async fn delete_file(&self, code: &str, filename: &str) -> Result<()> {
        let key = format!("{}/{}", code, filename);

        tracing::info!(
            "Deleting from S3: bucket={}, key={}",
            self.bucket_name,
            key
        );

        self.client
            .delete_object()
            .bucket(&self.bucket_name)
            .key(&key)
            .send()
            .await?;

        tracing::info!("Successfully deleted {} from S3", key);
        Ok(())
    }
}
