// Div owns exporting `quiz` from the shared crate root. This test-only module
// keeps the domain quiz tests runnable before that export is merged.
#[allow(dead_code, unused_imports)]
#[path = "../src/quiz/mod.rs"]
mod quiz;
