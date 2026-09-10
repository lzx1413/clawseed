//! Query evidence on a 0..=1 scale, independent of BM25 and fusion rank scores.

pub struct LexicalQuery {
    terms: Vec<String>,
}

impl LexicalQuery {
    pub fn new(query: &str) -> Self {
        let mut terms = query
            .split(|c: char| !c.is_alphanumeric())
            .map(str::to_lowercase)
            // Single characters and English function words are insufficient evidence
            // for automatically injecting a memory. Explicit unfiltered search still works.
            .filter(|term| {
                term.chars().count() >= 2
                    && !matches!(
                        term.as_str(),
                        "the"
                            | "an"
                            | "and"
                            | "or"
                            | "of"
                            | "to"
                            | "in"
                            | "on"
                            | "at"
                            | "is"
                            | "are"
                            | "was"
                            | "be"
                            | "it"
                            | "this"
                            | "that"
                            | "my"
                            | "me"
                            | "you"
                            | "your"
                            | "we"
                            | "our"
                            | "do"
                            | "does"
                            | "can"
                            | "could"
                            | "would"
                            | "please"
                            | "what"
                            | "which"
                            | "how"
                            | "tell"
                            | "about"
                    )
            })
            .collect::<Vec<_>>();
        terms.sort();
        terms.dedup();
        Self { terms }
    }

    pub fn score(&self, key: &str, content: &str) -> f64 {
        if self.terms.is_empty() {
            return 0.0;
        }
        let text = format!("{key}\n{content}").to_lowercase();
        let words = text
            .split(|c: char| !c.is_alphanumeric())
            .collect::<Vec<_>>();
        let matched = self.terms.iter().filter(|term| {
            // FTS unicode61 does not segment Chinese/Japanese. Allow meaningful
            // multi-character phrases inside these unsegmented words, but never
            // match Latin fragments such as "car" in "cartography".
            if term.chars().any(|c| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}' | '\u{20000}'..='\u{2fa1f}')) {
                text.contains(term.as_str())
            } else {
                words.contains(&term.as_str())
            }
        }).count();
        #[allow(clippy::cast_precision_loss)]
        {
            matched as f64 / self.terms.len() as f64
        }
    }
}
