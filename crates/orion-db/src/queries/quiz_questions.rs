use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::models::{NewQuizQuestion, QuizOption, QuizQuestion, QuizQuestionWithOptions, QuizType};

const QUESTION_BY_ID: &str = r#"
    SELECT
        id,
        quiz_type,
        category,
        question_text,
        explanation,
        active,
        created_at,
        updated_at
    FROM quiz_questions
    WHERE id = $1
      AND active = TRUE
"#;

const OPTIONS_BY_QUESTION_ID: &str = r#"
    SELECT
        id,
        question_id,
        option_text,
        position,
        is_correct,
        created_at
    FROM quiz_options
    WHERE question_id = $1
    ORDER BY position ASC, id ASC
"#;

const QUESTIONS_BY_TYPE: &str = r#"
    SELECT
        id,
        quiz_type,
        category,
        question_text,
        explanation,
        active,
        created_at,
        updated_at
    FROM quiz_questions
    WHERE active = TRUE
      AND quiz_type = $1
    ORDER BY id ASC
    LIMIT $2
    OFFSET $3
"#;

const RANDOM_QUESTIONS_BY_TYPE: &str = r#"
    SELECT
        id,
        quiz_type,
        category,
        question_text,
        explanation,
        active,
        created_at,
        updated_at
    FROM quiz_questions
    WHERE active = TRUE
      AND quiz_type = $1
    ORDER BY random()
    LIMIT $2
"#;

/// Returns one active question without answer options.
pub async fn find_by_id(pool: &PgPool, question_id: Uuid) -> Result<Option<QuizQuestion>> {
    sqlx::query_as::<_, QuizQuestion>(QUESTION_BY_ID)
        .bind(question_id)
        .fetch_optional(pool)
        .await
}

/// Returns all options in stable display order.
pub async fn options_by_question_id(pool: &PgPool, question_id: Uuid) -> Result<Vec<QuizOption>> {
    sqlx::query_as::<_, QuizOption>(OPTIONS_BY_QUESTION_ID)
        .bind(question_id)
        .fetch_all(pool)
        .await
}

/// Returns one question with its options and current question Elo.
pub async fn find_with_options(
    pool: &PgPool,
    question_id: Uuid,
) -> Result<Option<QuizQuestionWithOptions>> {
    let Some(question) = find_by_id(pool, question_id).await? else {
        return Ok(None);
    };
    let options = options_by_question_id(pool, question_id).await?;
    let rating = sqlx::query_as(
        r#"
        SELECT question_id, rating, attempts, correct_answers, created_at, updated_at
        FROM question_ratings
        WHERE question_id = $1
        "#,
    )
    .bind(question_id)
    .fetch_optional(pool)
    .await?;

    Ok(Some(QuizQuestionWithOptions {
        question,
        options,
        rating,
    }))
}

/// Returns a page of active questions for a quiz mode.
pub async fn list_by_type(
    pool: &PgPool,
    quiz_type: QuizType,
    limit: i64,
    offset: i64,
) -> Result<Vec<QuizQuestion>> {
    sqlx::query_as::<_, QuizQuestion>(QUESTIONS_BY_TYPE)
        .bind(quiz_type.as_str())
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

pub async fn basic_questions(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<QuizQuestion>> {
    list_by_type(pool, QuizType::Basic, limit, offset).await
}

/// Returns a paged set of active Basic Quiz questions with their options and
/// current question ratings.
pub async fn basic_questions_with_options(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<QuizQuestionWithOptions>> {
    questions_with_options_by_type(pool, QuizType::Basic, limit, offset).await
}

/// Returns a paged set of active Advanced Quiz questions with their options
/// and current question ratings.
pub async fn advanced_questions_with_options(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<QuizQuestionWithOptions>> {
    questions_with_options_by_type(pool, QuizType::Advanced, limit, offset).await
}

async fn questions_with_options_by_type(
    pool: &PgPool,
    quiz_type: QuizType,
    limit: i64,
    offset: i64,
) -> Result<Vec<QuizQuestionWithOptions>> {
    let questions = list_by_type(pool, quiz_type, limit, offset).await?;
    load_options_and_ratings(pool, questions).await
}

pub async fn advanced_questions(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<QuizQuestion>> {
    list_by_type(pool, QuizType::Advanced, limit, offset).await
}

/// Returns a random set of active questions for a quiz mode.
pub async fn random_by_type(
    pool: &PgPool,
    quiz_type: QuizType,
    limit: i64,
) -> Result<Vec<QuizQuestion>> {
    sqlx::query_as::<_, QuizQuestion>(RANDOM_QUESTIONS_BY_TYPE)
        .bind(quiz_type.as_str())
        .bind(limit)
        .fetch_all(pool)
        .await
}

/// Returns a random set of questions with their answer options and ratings.
pub async fn random_with_options_by_type(
    pool: &PgPool,
    quiz_type: QuizType,
    limit: i64,
) -> Result<Vec<QuizQuestionWithOptions>> {
    let questions = random_by_type(pool, quiz_type, limit).await?;
    load_options_and_ratings(pool, questions).await
}

/// Returns a random set of active Basic Quiz questions with their options.
pub async fn random_basic_questions_with_options(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<QuizQuestionWithOptions>> {
    random_with_options_by_type(pool, QuizType::Basic, limit).await
}

async fn load_options_and_ratings(
    pool: &PgPool,
    questions: Vec<QuizQuestion>,
) -> Result<Vec<QuizQuestionWithOptions>> {
    let mut result = Vec::with_capacity(questions.len());
    for question in questions {
        let options = options_by_question_id(pool, question.id).await?;
        let rating = sqlx::query_as(
            r#"
            SELECT question_id, rating, attempts, correct_answers, created_at, updated_at
            FROM question_ratings
            WHERE question_id = $1
            "#,
        )
        .bind(question.id)
        .fetch_optional(pool)
        .await?;
        result.push(QuizQuestionWithOptions {
            question,
            options,
            rating,
        });
    }
    Ok(result)
}

/// Inserts a question and all of its options atomically.
pub async fn insert(pool: &PgPool, question: &NewQuizQuestion) -> Result<QuizQuestionWithOptions> {
    let mut transaction = pool.begin().await?;

    let inserted = sqlx::query_as::<_, QuizQuestion>(
        r#"
        INSERT INTO quiz_questions (id, quiz_type, category, question_text, explanation)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, quiz_type, category, question_text, explanation, active, created_at, updated_at
        "#,
    )
    .bind(question.id)
    .bind(question.quiz_type.as_str())
    .bind(&question.category)
    .bind(&question.question_text)
    .bind(&question.explanation)
    .fetch_one(&mut *transaction)
    .await?;

    for option in &question.options {
        sqlx::query(
            r#"
            INSERT INTO quiz_options (id, question_id, option_text, position, is_correct)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(option.id)
        .bind(question.id)
        .bind(&option.option_text)
        .bind(option.position)
        .bind(option.is_correct)
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;

    Ok(QuizQuestionWithOptions {
        question: inserted,
        options: question
            .options
            .iter()
            .map(|option| QuizOption {
                id: option.id,
                question_id: question.id,
                option_text: option.option_text.clone(),
                position: option.position,
                is_correct: option.is_correct,
                created_at: chrono::Utc::now(),
            })
            .collect(),
        rating: None,
    })
}
