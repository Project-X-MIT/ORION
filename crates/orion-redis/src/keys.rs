use std::fmt;

use orion_domain::UserId;
use uuid::Uuid;

pub const ROOT_NAMESPACE: &str = "orion:v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedisNamespace {
    Session,
    RateLimit,
    Lock,
    Cache,
    PubSub,
}

impl RedisNamespace {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::RateLimit => "rate_limit",
            Self::Lock => "lock",
            Self::Cache => "cache",
            Self::PubSub => "pubsub",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedisTtl {
    Seconds(u64),
    Configured(&'static str),
    Lease,
    PersistentChannel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedisKeySpec {
    pub id: &'static str,
    pub pattern: &'static str,
    pub namespace: RedisNamespace,
    pub owner: &'static str,
    pub ttl: RedisTtl,
    pub invalidation_rule: &'static str,
}

macro_rules! spec {
    ($id:literal, $pattern:literal, $namespace:ident, $owner:literal, $ttl:expr, $rule:literal) => {
        RedisKeySpec {
            id: $id,
            pattern: $pattern,
            namespace: RedisNamespace::$namespace,
            owner: $owner,
            ttl: $ttl,
            invalidation_rule: $rule,
        }
    };
}

pub const REDIS_KEY_REGISTRY: &[RedisKeySpec] = &[
    spec!(
        "session",
        "orion:v1:session:{session_id}",
        Session,
        "divi912",
        RedisTtl::Configured("SESSION_TTL_SECONDS"),
        "Delete on logout or revocation; TTL handles expiration."
    ),
    spec!(
        "rate_limit.login",
        "orion:v1:rate_limit:login:{subject_hash}",
        RateLimit,
        "divi912",
        RedisTtl::Seconds(900),
        "Expire automatically; delete only for an administrative reset."
    ),
    spec!(
        "lock.advanced_settlement",
        "orion:v1:lock:advanced_settlement:{attempt_id}",
        Lock,
        "akaidk",
        RedisTtl::Lease,
        "Release after settlement; lease expiry recovers abandoned work."
    ),
    spec!(
        "lock.worker_job",
        "orion:v1:lock:worker_job:{job_name}",
        Lock,
        "divi912",
        RedisTtl::Lease,
        "Release after the run; lease expiry recovers abandoned work."
    ),
    spec!(
        "cache.quiz_question",
        "orion:v1:cache:quiz_question:{question_id}",
        Cache,
        "akaidk",
        RedisTtl::Seconds(300),
        "Delete after a question mutation or administrative disable."
    ),
    spec!(
        "cache.leaderboard",
        "orion:v1:cache:leaderboard:{limit}:{offset}",
        Cache,
        "ShauryaBijalwan",
        RedisTtl::Seconds(60),
        "Delete after committed rating changes or snapshot refresh."
    ),
    spec!(
        "cache.profile",
        "orion:v1:cache:profile:{user_id}",
        Cache,
        "ShauryaBijalwan",
        RedisTtl::Seconds(120),
        "Delete after committed profile, rating, or rank changes."
    ),
    spec!(
        "cache.research",
        "orion:v1:cache:research:{research_id}",
        Cache,
        "shivanshrawat13aug2007-commits",
        RedisTtl::Seconds(300),
        "Delete after publication changes; drafts are never cached here."
    ),
    spec!(
        "cache.news_feed",
        "orion:v1:cache:news_feed:{limit}:{offset}",
        Cache,
        "sudhanshu001122",
        RedisTtl::Seconds(120),
        "Delete after a successful ingestion transaction."
    ),
    spec!(
        "cache.learning_course",
        "orion:v1:cache:learning_course:{course_id}",
        Cache,
        "sudhanshu001122",
        RedisTtl::Seconds(3600),
        "Delete after a committed course-content update."
    ),
    spec!(
        "pubsub.notification",
        "orion:v1:pubsub:notification",
        PubSub,
        "divi912",
        RedisTtl::PersistentChannel,
        "Channels carry ephemeral hints; durable delivery comes from PostgreSQL/outbox."
    ),
    spec!(
        "pubsub.rating",
        "orion:v1:pubsub:rating",
        PubSub,
        "akaidk",
        RedisTtl::PersistentChannel,
        "Channels carry ephemeral hints; durable rating state remains in PostgreSQL."
    ),
];

#[must_use]
pub fn redis_key(id: &str) -> Option<&'static RedisKeySpec> {
    REDIS_KEY_REGISTRY.iter().find(|entry| entry.id == id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedisKey {
    Session { session_id: Uuid },
    AdvancedSettlementLock { attempt_id: Uuid },
    WorkerJobLock { job_name: String },
    QuizQuestion { question_id: Uuid },
    Leaderboard { limit: u32, offset: u64 },
    Profile { user_id: UserId },
    Research { research_id: Uuid },
    NewsFeed { limit: u32, offset: u64 },
    LearningCourse { course_id: Uuid },
}

impl fmt::Display for RedisKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session { session_id } => {
                write!(formatter, "{ROOT_NAMESPACE}:session:{session_id}")
            }
            Self::AdvancedSettlementLock { attempt_id } => write!(
                formatter,
                "{ROOT_NAMESPACE}:lock:advanced_settlement:{attempt_id}"
            ),
            Self::WorkerJobLock { job_name } => {
                write!(formatter, "{ROOT_NAMESPACE}:lock:worker_job:{job_name}")
            }
            Self::QuizQuestion { question_id } => {
                write!(
                    formatter,
                    "{ROOT_NAMESPACE}:cache:quiz_question:{question_id}"
                )
            }
            Self::Leaderboard { limit, offset } => write!(
                formatter,
                "{ROOT_NAMESPACE}:cache:leaderboard:{limit}:{offset}"
            ),
            Self::Profile { user_id } => {
                write!(formatter, "{ROOT_NAMESPACE}:cache:profile:{user_id}")
            }
            Self::Research { research_id } => {
                write!(formatter, "{ROOT_NAMESPACE}:cache:research:{research_id}")
            }
            Self::NewsFeed { limit, offset } => {
                write!(
                    formatter,
                    "{ROOT_NAMESPACE}:cache:news_feed:{limit}:{offset}"
                )
            }
            Self::LearningCourse { course_id } => write!(
                formatter,
                "{ROOT_NAMESPACE}:cache:learning_course:{course_id}"
            ),
        }
    }
}
