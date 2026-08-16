use std::future::Future;

use orion_db::models::QuizQuestionWithOptions;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{RedisClient, RedisClientError, RedisKey};

/// The question cache is disposable and must never outlive the registered
/// `cache.quiz_question` contract.
pub const QUESTION_CACHE_TTL_SECONDS: i64 = 300;
pub const QUESTION_CACHE_SCHEMA_VERSION: u16 = 1;

/// A safe read projection of a quiz question.  In particular, `is_correct`
/// is intentionally absent so a cache read cannot become an answer-key leak.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedQuestion {
    pub schema_version: u16,
    pub id: Uuid,
    pub quiz_type: String,
    pub category: String,
    pub question_text: String,
    pub explanation: Option<String>,
    pub active: bool,
    pub options: Vec<CachedQuestionOption>,
    pub rating: Option<CachedQuestionRating>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedQuestionOption {
    pub id: Uuid,
    pub option_text: String,
    pub position: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedQuestionRating {
    pub question_id: Uuid,
    pub rating: i32,
    pub attempts: i32,
    pub correct_answers: i32,
}

impl CachedQuestion {
    /// Builds the cache projection from the authoritative DB read model.
    #[must_use]
    pub fn from_authoritative(value: &QuizQuestionWithOptions) -> Self {
        Self {
            schema_version: QUESTION_CACHE_SCHEMA_VERSION,
            id: value.question.id,
            quiz_type: value.question.quiz_type.clone(),
            category: value.question.category.clone(),
            question_text: value.question.question_text.clone(),
            explanation: value.question.explanation.clone(),
            active: value.question.active,
            options: value
                .options
                .iter()
                .map(|option| CachedQuestionOption {
                    id: option.id,
                    option_text: option.option_text.clone(),
                    position: option.position,
                })
                .collect(),
            rating: value.rating.map(|rating| CachedQuestionRating {
                question_id: rating.question_id,
                rating: rating.rating,
                attempts: rating.attempts,
                correct_answers: rating.correct_answers,
            }),
        }
    }
}

#[derive(Debug, Error)]
pub enum QuestionCacheError {
    #[error("question cache Redis operation failed")]
    Redis(#[from] RedisClientError),
    #[error("question cache payload serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error("question cache schema version is unsupported")]
    UnsupportedSchemaVersion,
}

#[derive(Clone)]
pub struct QuestionCache {
    client: RedisClient,
}

impl QuestionCache {
    #[must_use]
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    /// Reads a question cache entry. A missing entry is a normal cache miss;
    /// Redis failures remain visible to callers that want to record telemetry.
    pub async fn get(
        &self,
        question_id: Uuid,
    ) -> Result<Option<CachedQuestion>, QuestionCacheError> {
        let key = question_key(question_id);
        let Some(payload) = self.client.get(key.clone()).await? else {
            return Ok(None);
        };

        let entry = match serde_json::from_str::<CachedQuestion>(&payload) {
            Ok(entry) => entry,
            Err(error) => {
                let _ = self.client.delete(key).await;
                return Err(error.into());
            }
        };
        if entry.schema_version != QUESTION_CACHE_SCHEMA_VERSION {
            let _ = self.client.delete(key).await;
            return Err(QuestionCacheError::UnsupportedSchemaVersion);
        }
        if entry.id != question_id || !entry.active {
            let _ = self.client.delete(key).await;
            return Ok(None);
        }
        Ok(Some(entry))
    }

    /// Stores a read projection with the registered question-cache TTL.
    pub async fn put(
        &self,
        question_id: Uuid,
        value: &CachedQuestion,
    ) -> Result<(), QuestionCacheError> {
        if value.schema_version != QUESTION_CACHE_SCHEMA_VERSION || value.id != question_id {
            return Err(QuestionCacheError::UnsupportedSchemaVersion);
        }
        let payload = serde_json::to_string(value)?;
        self.client
            .set_ex(
                question_key(question_id),
                payload,
                QUESTION_CACHE_TTL_SECONDS,
            )
            .await?;
        Ok(())
    }

    /// Deletes a question after a committed question/options mutation. Redis
    /// deletion is idempotent and expiry remains the safety net.
    pub async fn invalidate(&self, question_id: Uuid) -> Result<(), QuestionCacheError> {
        self.client.delete(question_key(question_id)).await?;
        Ok(())
    }

    /// Rebuilds question entries from an authoritative PostgreSQL snapshot.
    /// The snapshot is converted before any Redis write, so answer keys are
    /// never introduced into the disposable cache during a rebuild.
    pub async fn rebuild(
        &self,
        authoritative: impl IntoIterator<Item = QuizQuestionWithOptions>,
    ) -> Result<usize, QuestionCacheError> {
        let mut rebuilt = 0;
        for question in authoritative {
            let projection = CachedQuestion::from_authoritative(&question);
            self.put(question.question.id, &projection).await?;
            rebuilt += 1;
        }
        Ok(rebuilt)
    }

    /// Cache-first read with an authoritative loader fallback. Redis errors,
    /// corrupt values, and misses all fall through to the loader; a failed
    /// best-effort cache fill never hides a successful DB read.
    pub async fn get_or_load<F, Fut, E>(
        &self,
        question_id: Uuid,
        loader: F,
    ) -> Result<CachedQuestion, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<QuizQuestionWithOptions, E>>,
    {
        let value = load_after_cache_read(self.get(question_id).await.map_err(|_| ()), || async {
            let authoritative = loader().await?;
            Ok::<_, E>(CachedQuestion::from_authoritative(&authoritative))
        })
        .await?;
        let _ = self.put(question_id, &value).await;
        Ok(value)
    }
}

async fn load_after_cache_read<T, F, Fut, E>(
    cache_result: Result<Option<T>, ()>,
    loader: F,
) -> Result<T, E>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    if let Ok(Some(value)) = cache_result {
        return Ok(value);
    }
    loader().await
}

fn question_key(question_id: Uuid) -> String {
    // Keep key construction centralized in the shared registry-backed key
    // type; this prevents feature-local prefixes from drifting.
    RedisKey::QuizQuestion { question_id }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_does_not_contain_answer_key_fields() {
        let value = serde_json::to_value(CachedQuestion {
            schema_version: QUESTION_CACHE_SCHEMA_VERSION,
            id: Uuid::nil(),
            quiz_type: "basic".to_owned(),
            category: "test".to_owned(),
            question_text: "Question".to_owned(),
            explanation: None,
            active: true,
            options: vec![CachedQuestionOption {
                id: Uuid::from_u128(1),
                option_text: "Option".to_owned(),
                position: 0,
            }],
            rating: None,
        })
        .expect("cache projection serializes");

        assert!(value.get("is_correct").is_none());
        assert!(value["options"][0].get("is_correct").is_none());
    }

    #[test]
    fn question_key_uses_the_registered_pattern_and_ttl() {
        assert_eq!(
            crate::redis_key("cache.quiz_question").map(|spec| spec.ttl),
            Some(crate::RedisTtl::Seconds(QUESTION_CACHE_TTL_SECONDS as u64))
        );
        assert_eq!(
            question_key(Uuid::nil()),
            "orion:v1:cache:quiz_question:00000000-0000-0000-0000-000000000000"
        );
    }

    #[tokio::test]
    async fn redis_failure_falls_back_to_the_authoritative_loader() {
        let loaded = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let loaded_by_db = std::sync::Arc::clone(&loaded);
        let result: Result<CachedQuestion, &str> = load_after_cache_read(Err(()), || async move {
            loaded_by_db.store(true, std::sync::atomic::Ordering::SeqCst);
            Err("PostgreSQL remains the source of truth")
        })
        .await;

        assert_eq!(result, Err("PostgreSQL remains the source of truth"));
        assert!(loaded.load(std::sync::atomic::Ordering::SeqCst));
    }
}
