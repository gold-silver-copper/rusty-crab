//! Structural coverage of reviewed Reference productions. This parses the small
//! EBNF core used by the mapped productions, not Rust source. rustc supplies all
//! source witnesses. Unhandled Reference notation must never silently disappear.
use anyhow::{Result, bail, ensure};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Terminal(String),
    Reference(String),
    Sequence(Vec<Node>),
    Choice(Vec<Node>),
    Optional(Box<Node>),
    Repeat(Box<Node>, bool), // true means one-or-more, false means zero-or-more
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Text(String),
    Name(String),
    Open,
    Close,
    Or,
    Optional,
    Star,
    Plus,
}

fn tokens(input: &str) -> Result<Vec<Token>> {
    let mut out = Vec::new();
    let mut remaining = input;
    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }
        if remaining.starts_with("//") {
            remaining = remaining.split_once('\n').map_or("", |(_, rest)| rest);
            continue;
        }
        let c = remaining.chars().next().unwrap();
        match c {
            '`' => {
                let Some(end) = remaining[1..].find('`') else {
                    bail!("unterminated grammar terminal")
                };
                out.push(Token::Text(remaining[1..end + 1].into()));
                remaining = &remaining[end + 2..];
            }
            '(' | ')' | '|' | '?' | '*' | '+' => {
                out.push(match c {
                    '(' => Token::Open,
                    ')' => Token::Close,
                    '|' => Token::Or,
                    '?' => Token::Optional,
                    '*' => Token::Star,
                    '+' => Token::Plus,
                    _ => unreachable!(),
                });
                remaining = &remaining[1..];
            }
            c if c.is_ascii_alphabetic() => {
                let end = remaining
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .unwrap_or(remaining.len());
                out.push(Token::Name(remaining[..end].into()));
                remaining = &remaining[end..];
            }
            _ => bail!(
                "unhandled Reference notation starting at {:?}",
                remaining.chars().take(40).collect::<String>()
            ),
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}
impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }
    fn choice(&mut self) -> Result<Node> {
        let mut alternatives = vec![self.sequence()?];
        while self.peek() == Some(&Token::Or) {
            self.cursor += 1;
            alternatives.push(self.sequence()?);
        }
        Ok(if alternatives.len() == 1 {
            alternatives.pop().unwrap()
        } else {
            Node::Choice(alternatives)
        })
    }
    fn sequence(&mut self) -> Result<Node> {
        let mut nodes = Vec::new();
        while !matches!(self.peek(), None | Some(Token::Close | Token::Or)) {
            let token = self.peek().unwrap().clone();
            self.cursor += 1;
            let node = match token {
                Token::Text(text) => Node::Terminal(text),
                Token::Name(name) => Node::Reference(name),
                Token::Open => {
                    let node = self.choice()?;
                    ensure!(
                        self.peek() == Some(&Token::Close),
                        "unclosed Reference grammar group"
                    );
                    self.cursor += 1;
                    node
                }
                _ => bail!("unexpected grammar token {token:?}"),
            };
            let node = match self.peek() {
                Some(Token::Optional) => {
                    self.cursor += 1;
                    Node::Optional(Box::new(node))
                }
                Some(Token::Star) => {
                    self.cursor += 1;
                    Node::Repeat(Box::new(node), false)
                }
                Some(Token::Plus) => {
                    self.cursor += 1;
                    Node::Repeat(Box::new(node), true)
                }
                _ => node,
            };
            nodes.push(node);
        }
        ensure!(!nodes.is_empty(), "empty grammar sequence or alternative");
        Ok(if nodes.len() == 1 {
            nodes.pop().unwrap()
        } else {
            Node::Sequence(nodes)
        })
    }
}

pub fn forms(production: &str) -> Result<BTreeMap<String, String>> {
    let (_, body) = production
        .split_once(" ->")
        .ok_or_else(|| anyhow::anyhow!("missing grammar arrow"))?;
    let mut parser = Parser {
        tokens: tokens(body)?,
        cursor: 0,
    };
    let tree = parser.choice()?;
    ensure!(
        parser.cursor == parser.tokens.len(),
        "trailing grammar tokens"
    );
    let mut forms = BTreeMap::from([("production".into(), "A compiler-accepted source instance of this production; referenced productions have their own requirements.".into())]);
    fn visit(node: &Node, path: &str, forms: &mut BTreeMap<String, String>) {
        match node {
            Node::Choice(alternatives) => {
                for (i, alternative) in alternatives.iter().enumerate() {
                    let key = format!("{path}.alternative.{i}");
                    forms.insert(
                        key.clone(),
                        format!(
                            "Select grammar alternative {} at {path}: {alternative:?}",
                            i + 1
                        ),
                    );
                    visit(alternative, &key, forms);
                }
            }
            Node::Sequence(nodes) => {
                for (i, node) in nodes.iter().enumerate() {
                    visit(node, &format!("{path}.sequence.{i}"), forms);
                }
            }
            Node::Optional(node) => {
                for form in ["absent", "present"] {
                    forms.insert(
                        format!("{path}.{form}"),
                        format!("Optional grammar group at {path} is {form}: {node:?}"),
                    );
                }
                visit(node, &format!("{path}.body"), forms);
            }
            Node::Repeat(node, one_or_more) => {
                for form in if *one_or_more {
                    &["one", "many"][..]
                } else {
                    &["zero", "one", "many"][..]
                } {
                    forms.insert(format!("{path}.{form}"), format!("Repeated grammar group at {path} has {form} occurrences (many means at least two): {node:?}"));
                }
                visit(node, &format!("{path}.body"), forms);
            }
            Node::Terminal(_) | Node::Reference(_) => {}
        }
    }
    visit(&tree, "root", &mut forms);
    Ok(forms)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alternatives_optionals_and_repetition_are_separate_requirements() {
        let forms = forms("Example -> (`left` Name? | `right` Name*) (`tail`)+").unwrap();
        for key in [
            "production",
            "root.sequence.0.alternative.0",
            "root.sequence.0.alternative.1",
            "root.sequence.0.alternative.0.sequence.1.absent",
            "root.sequence.0.alternative.0.sequence.1.present",
            "root.sequence.0.alternative.1.sequence.1.zero",
            "root.sequence.0.alternative.1.sequence.1.one",
            "root.sequence.0.alternative.1.sequence.1.many",
            "root.sequence.1.one",
            "root.sequence.1.many",
        ] {
            assert!(forms.contains_key(key), "{key}");
        }
        assert_eq!(forms.len(), 10);
    }
    #[test]
    fn terminals_are_not_meta_syntax_and_unknown_notation_is_rejected() {
        assert_eq!(
            forms("P -> `|` `?` `*` `+` `(` `)` `//` // comment\n Name")
                .unwrap()
                .len(),
            1
        );
        for source in [
            "P -> A |",
            "P -> (A",
            "P -> A)",
            "P -> A**",
            "P -> ~`x`",
            "P -> A _except [B]_",
            "P -> `unclosed",
            "P -> A{2}",
        ] {
            assert!(forms(source).is_err(), "{source}");
        }
    }
}
