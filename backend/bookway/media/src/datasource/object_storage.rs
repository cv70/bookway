use super::*;

#[derive(Clone)]
pub(crate) struct ObjectStorage {
    bucket: Bucket,
    credentials: Credentials,
    client: reqwest::Client,
}

impl ObjectStorage {
    pub(crate) fn new(
        endpoint: &str,
        bucket: String,
        region: String,
        key: String,
        secret: String,
    ) -> Result<Self, ObjectError> {
        let bucket = Bucket::new(endpoint.parse()?, UrlStyle::Path, bucket, region)
            .map_err(|error| ObjectError::Bucket(error.to_string()))?;
        Ok(Self {
            bucket,
            credentials: Credentials::new(key, secret),
            client: bookway_runtime::http_client(),
        })
    }
    pub(crate) fn presign_put(&self, key: &str) -> String {
        self.bucket
            .put_object(Some(&self.credentials), key)
            .sign(Duration::from_secs(900))
            .to_string()
    }
    pub(crate) async fn metadata(&self, key: &str) -> Result<Option<ObjectMetadata>, ObjectError> {
        let response = self
            .client
            .head(
                self.bucket
                    .head_object(Some(&self.credentials), key)
                    .sign(Duration::from_secs(60)),
            )
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let response = response.error_for_status()?;
        let size_bytes = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| ObjectError::InvalidMetadata("missing content-length".to_string()))?;
        let mime_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| {
                value
                    .split(';')
                    .next()
                    .unwrap_or(value)
                    .trim()
                    .to_ascii_lowercase()
            });
        Ok(Some(ObjectMetadata {
            size_bytes,
            mime_type,
        }))
    }
    pub(crate) async fn upload(
        &self,
        key: &str,
        mime: &str,
        body: axum::body::Bytes,
    ) -> Result<(), ObjectError> {
        let response = self
            .client
            .put(self.presign_put(key))
            .header("content-type", mime)
            .body(body)
            .send()
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(ObjectError::Rejected)
        }
    }
}
