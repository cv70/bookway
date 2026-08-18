use std::{env, time::Duration};

use bookway_bbs_link_api::pb::{self as content_pb, bbs_link_client::BbsLinkClient};
use bookway_growth_api::pb as growth_pb;
use serde_json::Value;
use uuid::Uuid;

const BATCH_SIZE: i64 = 100;
const JOB_LEASE_SECONDS: i32 = 300;
const MAX_ATTEMPTS: i32 = 10;

struct PublicationJob {
    entry_id: String,
    user_id: String,
    payload: Value,
    lease_id: Uuid,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("entry-publication-dispatcher");
    let pool = bookway_data::postgres_pool().await?;
    let content_url =
        env::var("BBS_LINK_GRPC_URL").unwrap_or_else(|_| "http://127.0.0.1:18004".to_string());
    let client = BbsLinkClient::connect(content_url).await?;

    loop {
        let jobs = match claim_jobs(&pool).await {
            Ok(jobs) => jobs,
            Err(error) => {
                tracing::error!(%error, "could not claim entry publication jobs");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        if jobs.is_empty() {
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }
        for job in jobs {
            if let Err(error) = publish_entry(&client, &pool, &job).await {
                tracing::warn!(entry_id = %job.entry_id, %error, "entry publication failed");
                match schedule_retry(&pool, &job, &error).await {
                    Ok(true) => {}
                    Ok(false) => {
                        tracing::debug!(entry_id = %job.entry_id, "entry publication lease was replaced before retry")
                    }
                    Err(retry_error) => {
                        tracing::error!(entry_id = %job.entry_id, %retry_error, "could not schedule entry publication retry")
                    }
                }
            }
        }
    }
}

async fn claim_jobs(pool: &sqlx::PgPool) -> Result<Vec<PublicationJob>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let rows = sqlx::query_as::<_, (String, String, Value, Uuid)>(
        r#"
        WITH claimed AS (
            SELECT entry_id
            FROM entry_publication_jobs
            WHERE (status = 'pending' AND available_at <= now())
               OR (
                    status = 'processing'
                    AND COALESCE(locked_at, created_at) <= now() - make_interval(secs => $2)
               )
            ORDER BY available_at, created_at
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        )
        UPDATE entry_publication_jobs AS job
        SET status = 'processing',
            attempts = job.attempts + 1,
            locked_at = now(),
            lease_id = gen_random_uuid(),
            updated_at = now()
        FROM claimed
        WHERE job.entry_id = claimed.entry_id
        RETURNING job.entry_id, job.user_id, job.payload, job.lease_id
        "#,
    )
    .bind(BATCH_SIZE)
    .bind(JOB_LEASE_SECONDS)
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(rows
        .into_iter()
        .map(|(entry_id, user_id, payload, lease_id)| PublicationJob {
            entry_id,
            user_id,
            payload,
            lease_id,
        })
        .collect())
}

async fn publish_entry(
    client: &BbsLinkClient<tonic::transport::Channel>,
    pool: &sqlx::PgPool,
    job: &PublicationJob,
) -> Result<(), String> {
    let request = content_request(job)?;
    let mut client = client.clone();
    let created = client
        .create(bookway_runtime::grpc_service_request(request).map_err(|error| error.to_string())?)
        .await
        .map_err(|error| error.to_string())?
        .into_inner();
    // Create is idempotent by entry ID. Do not re-run moderation if a prior
    // delivery reached BBS Link but this worker crashed before acknowledgement.
    let content = if created.status == content_pb::ContentStatus::Draft as i32 {
        client
            .publish(
                bookway_runtime::grpc_service_request(content_pb::PublishRequest {
                    user_id: job.user_id.clone(),
                    id: created.id.clone(),
                    idempotency_key: Some(format!("entry-publish:{}", job.entry_id)),
                })
                .map_err(|error| error.to_string())?,
            )
            .await
            .map_err(|error| error.to_string())?
            .into_inner()
    } else {
        created
    };
    complete_publication(pool, job, &content).await
}

async fn complete_publication(
    pool: &sqlx::PgPool,
    job: &PublicationJob,
    content: &content_pb::Content,
) -> Result<(), String> {
    let (publication_status, published) = entry_publication_status(content.status)?;
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let owned = sqlx::query_scalar::<_, String>(
        "SELECT entry_id FROM entry_publication_jobs WHERE entry_id=$1 AND lease_id=$2 AND status='processing' FOR UPDATE",
    )
    .bind(&job.entry_id)
    .bind(job.lease_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    if owned.is_none() {
        return Err("entry publication lease was replaced before completion".to_string());
    }
    let payload = sqlx::query_scalar::<_, Value>(
        "SELECT payload FROM growth_entries WHERE id=$1 AND user_id=$2 FOR UPDATE",
    )
    .bind(&job.entry_id)
    .bind(&job.user_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "entry no longer exists".to_string())?;
    let mut entry: growth_pb::GrowthEntry =
        serde_json::from_value(payload).map_err(|error| error.to_string())?;
    entry.publication_status = publication_status;
    entry.published = published;
    entry.public_content_id = Some(content.id.clone());
    entry.publication_error = None;
    sqlx::query("UPDATE growth_entries SET payload=$3,published=$4 WHERE id=$1 AND user_id=$2")
        .bind(&job.entry_id)
        .bind(&job.user_id)
        .bind(serde_json::to_value(&entry).map_err(|error| error.to_string())?)
        .bind(published)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE entry_publication_jobs SET status='delivered',content_id=$3,locked_at=NULL,lease_id=NULL,last_error=NULL,updated_at=now() WHERE entry_id=$1 AND lease_id=$2 AND status='processing'",
    )
    .bind(&job.entry_id)
    .bind(job.lease_id)
    .bind(&content.id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())
}

async fn schedule_retry(
    pool: &sqlx::PgPool,
    job: &PublicationJob,
    error: &str,
) -> Result<bool, String> {
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let attempts = sqlx::query_scalar::<_, i32>(
        "SELECT attempts FROM entry_publication_jobs WHERE entry_id=$1 AND lease_id=$2 AND status='processing' FOR UPDATE",
    )
    .bind(&job.entry_id)
    .bind(job.lease_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    let Some(attempts) = attempts else {
        transaction
            .commit()
            .await
            .map_err(|error| error.to_string())?;
        return Ok(false);
    };
    let terminal = attempts >= MAX_ATTEMPTS;
    if terminal {
        let payload = sqlx::query_scalar::<_, Value>(
            "SELECT payload FROM growth_entries WHERE id=$1 AND user_id=$2 FOR UPDATE",
        )
        .bind(&job.entry_id)
        .bind(&job.user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
        if let Some(payload) = payload {
            let mut entry: growth_pb::GrowthEntry =
                serde_json::from_value(payload).map_err(|error| error.to_string())?;
            entry.published = false;
            entry.publication_status = growth_pb::EntryPublicationStatus::Failed as i32;
            entry.publication_error = Some(error.chars().take(300).collect());
            let entry_payload = serde_json::to_value(entry).map_err(|error| error.to_string())?;
            sqlx::query(
                "UPDATE growth_entries SET payload=$3,published=false WHERE id=$1 AND user_id=$2",
            )
            .bind(&job.entry_id)
            .bind(&job.user_id)
            .bind(entry_payload)
            .execute(&mut *transaction)
            .await
            .map_err(|error| error.to_string())?;
        }
    }
    sqlx::query(
        r#"
        UPDATE entry_publication_jobs
        SET status = CASE WHEN $3 THEN 'dead' ELSE 'pending' END,
            available_at = CASE
                WHEN $3 THEN now()
                ELSE now() + make_interval(
                    secs => LEAST(300, CAST(power(2, attempts) AS INTEGER))
                )
            END,
            locked_at = NULL,
            lease_id = NULL,
            last_error = left($4, 2000),
            updated_at = now()
        WHERE entry_id=$1 AND lease_id=$2 AND status='processing'
        "#,
    )
    .bind(&job.entry_id)
    .bind(job.lease_id)
    .bind(terminal)
    .bind(error)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok(true)
}

fn content_request(job: &PublicationJob) -> Result<content_pb::CreateRequest, String> {
    let user_id = required_string(&job.payload, "user_id")?;
    if user_id != job.user_id {
        return Err("publication payload user does not match its job".to_string());
    }
    let domain = match required_i32(&job.payload, "domain")? {
        0 => content_pb::GrowthDomain::Learning as i32,
        1 => content_pb::GrowthDomain::Movement as i32,
        2 => content_pb::GrowthDomain::Wellness as i32,
        3 => content_pb::GrowthDomain::Travel as i32,
        4 => content_pb::GrowthDomain::Leisure as i32,
        _ => return Err("publication payload has an invalid growth domain".to_string()),
    };
    Ok(content_pb::CreateRequest {
        user_id,
        idempotency_key: Some(required_string(&job.payload, "idempotency_key")?),
        title: required_string(&job.payload, "title")?,
        summary: required_string(&job.payload, "summary")?,
        body: required_string(&job.payload, "body")?,
        domain,
        content_type: content_pb::ContentType::Note as i32,
        cover_url: None,
        tags: Vec::new(),
        topics: Vec::new(),
        route_title: optional_string(&job.payload, "route_title"),
        route_duration: optional_string(&job.payload, "route_duration"),
        media_asset_ids: optional_string_list(&job.payload, "media_asset_ids")?,
        route_template: None,
        milestone: None,
        question_context: None,
    })
}

fn entry_publication_status(content_status: i32) -> Result<(i32, bool), String> {
    match content_pb::ContentStatus::try_from(content_status) {
        Ok(content_pb::ContentStatus::Published) => {
            Ok((growth_pb::EntryPublicationStatus::Published as i32, true))
        }
        Ok(content_pb::ContentStatus::Reviewing) => {
            Ok((growth_pb::EntryPublicationStatus::Reviewing as i32, false))
        }
        Ok(content_pb::ContentStatus::Restricted) => {
            Ok((growth_pb::EntryPublicationStatus::Restricted as i32, false))
        }
        Ok(content_pb::ContentStatus::Draft | content_pb::ContentStatus::Deleted) | Err(_) => {
            Err("content publication did not reach an auditable state".to_string())
        }
    }
}

fn required_string(payload: &Value, name: &str) -> Result<String, String> {
    payload
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("publication payload is missing {name}"))
}

fn optional_string(payload: &Value, name: &str) -> Option<String> {
    payload
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn optional_string_list(payload: &Value, name: &str) -> Result<Vec<String>, String> {
    let Some(value) = payload.get(name) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("publication payload {name} must be a list"))?
        .iter()
        .map(Value::as_str)
        .map(|value| {
            value
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("publication payload {name} contains an invalid value"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(values)
}

fn required_i32(payload: &Value, name: &str) -> Result<i32, String> {
    payload
        .get(name)
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| format!("publication payload is missing {name}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{PublicationJob, content_request, entry_publication_status};
    use bookway_bbs_link_api::pb as content_pb;
    use bookway_growth_api::pb as growth_pb;
    use uuid::Uuid;

    fn job(payload: serde_json::Value) -> PublicationJob {
        PublicationJob {
            entry_id: "entry-1".to_string(),
            user_id: "user-1".to_string(),
            payload,
            lease_id: Uuid::nil(),
        }
    }

    #[test]
    fn content_request_uses_stable_entry_idempotency_and_only_public_fields() {
        let request = content_request(&job(json!({
            "user_id": "user-1",
            "idempotency_key": "entry-publication:entry-1",
            "title": "A short reflection",
            "summary": "A public summary",
            "body": "The public body",
            "domain": 0,
            "media_asset_ids": ["0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b"],
            "route_title": null,
            "route_duration": null,
            "location": "private location",
            "mood": "private mood"
        })))
        .expect("valid publication payload");

        assert_eq!(
            request.idempotency_key.as_deref(),
            Some("entry-publication:entry-1")
        );
        assert_eq!(request.body, "The public body");
        assert!(request.cover_url.is_none());
        assert!(request.route_title.is_none());
        assert_eq!(
            request.media_asset_ids,
            vec!["0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b"]
        );
    }

    #[test]
    fn audit_outcomes_map_to_visible_entry_states() {
        assert_eq!(
            entry_publication_status(content_pb::ContentStatus::Published as i32),
            Ok((growth_pb::EntryPublicationStatus::Published as i32, true))
        );
        assert_eq!(
            entry_publication_status(content_pb::ContentStatus::Reviewing as i32),
            Ok((growth_pb::EntryPublicationStatus::Reviewing as i32, false))
        );
        assert_eq!(
            entry_publication_status(content_pb::ContentStatus::Restricted as i32),
            Ok((growth_pb::EntryPublicationStatus::Restricted as i32, false))
        );
    }
}
