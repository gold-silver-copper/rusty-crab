use crate::{model::Evidence, *};
use rustc_lexer::{FrontmatterAllowed, TokenKind, tokenize};

pub fn observe(file: &str, source: &str) -> (Vec<Evidence>, Vec<usize>) {
    let mut evidence = Vec::new();
    let mut annotated = vec![false; source.lines().count()];
    let shebang = rustc_lexer::strip_shebang(source).unwrap_or(0);
    let mut offset = shebang;
    for token in tokenize(&source[shebang..], FrontmatterAllowed::Yes) {
        let end = offset + token.len as usize;
        let text = &source[offset..end];
        let line = source[..offset].bytes().filter(|b| *b == b'\n').count() + 1;
        let column = source[..offset]
            .rsplit('\n')
            .next()
            .unwrap()
            .chars()
            .count()
            + 1;
        let mut ids = vec![classify_lex_TokenKind(&token.kind)];
        if matches!(token.kind, TokenKind::Ident | TokenKind::Lifetime { .. }) {
            if let Some((_, id)) = KEYWORDS.iter().find(|(spelling, _)| *spelling == text) {
                ids.push(id);
            }
        }
        if matches!(token.kind, TokenKind::Dollar) {
            let mut following = tokenize(&source[end..], FrontmatterAllowed::No);
            if let Some(next) = following.next() {
                if matches!(next.kind, TokenKind::Ident)
                    && &source[end..end + next.len as usize] == "crate"
                {
                    ids.push("keyword.DollarCrate");
                }
            }
        }
        if let TokenKind::Literal { kind, .. } = &token.kind {
            ids.push(classify_lex_LiteralKind(kind));
            match kind {
                rustc_lexer::LiteralKind::Int { base, .. }
                | rustc_lexer::LiteralKind::Float { base, .. } => ids.push(classify_lex_Base(base)),
                _ => {}
            }
        }
        match token.kind {
            TokenKind::LineComment { doc_style }
            | TokenKind::BlockComment {
                doc_style,
                terminated: true,
            } => {
                if let Some(style) = doc_style {
                    ids.push(classify_lex_DocStyle(&style));
                }
                // Delimiters alone are not an explanation. A real comment token is required.
                if text.chars().filter(|c| c.is_alphabetic()).count() >= 3 {
                    let last_line = line + text.bytes().filter(|b| *b == b'\n').count();
                    for n in line..=last_line {
                        if let Some(slot) = annotated.get_mut(n - 1) {
                            *slot = true;
                        }
                    }
                }
            }
            _ => {}
        }
        for id in ids {
            evidence.push(Evidence {
                requirement: id.into(),
                file: file.into(),
                start: offset,
                end,
                line,
                column,
                stage: "lexical_presence".into(),
                detector: "rustc_lexer_v1".into(),
                origin: "source".into(),
                configuration: String::new(),
                macro_backtrace: Vec::new(),
                witness: None,
            });
        }
        offset = end;
    }
    let missing = annotated
        .iter()
        .enumerate()
        .filter(|(_, a)| !**a)
        .map(|(i, _)| i + 1)
        .collect();
    (evidence, missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn string_contents_do_not_provide_comments_or_operators() {
        let (evidence, missing) = observe("main.rs", "fn main() { let _ = \"// loop +=\"; }\n");
        assert_eq!(missing, [1]);
        assert!(
            !evidence
                .iter()
                .any(|e| e.requirement == "lex.TokenKind.LineComment"
                    || e.requirement == "lex.TokenKind.Plus")
        );
    }
    #[test]
    fn block_and_doc_comments_are_real_annotations() {
        let (_, missing) = observe(
            "main.rs",
            "//! Crate documentation.\nfn main() { /* A function body. */ }\n",
        );
        assert!(missing.is_empty());
    }
}
