// Div owns `src/lib.rs`; compile the feature module directly until its public
// module export is registered through the shared contract registry.
#[path = "../src/news.rs"]
mod news;

#[test]
fn contract_version_is_explicit() {
    assert_eq!(news::NEWS_CONTRACT_VERSION, 1);
}

#[test]
fn future_publication_is_not_feed_eligible() {
    let now = chrono::Utc::now();
    let article = news::NewsArticle {
        id: uuid::Uuid::nil(),
        source_id: uuid::Uuid::nil(),
        external_id: None,
        title: "Markets open higher".to_owned(),
        summary: "A market summary".to_owned(),
        content: "Plain article content".to_owned(),
        url: "https://example.com/article".to_owned(),
        image_url: None,
        author: None,
        category: None,
        symbols: Vec::new(),
        published_at: now + chrono::Duration::minutes(1),
        ingested_at: now,
        created_at: now,
        updated_at: now,
    };

    assert_eq!(
        article.validate_for_feed_at(now),
        Err(news::NewsValidationError::FuturePublicationTimestamp)
    );
}
